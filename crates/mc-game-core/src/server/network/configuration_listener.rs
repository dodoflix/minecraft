//! `ServerConfigurationPacketListenerImpl`: the configuration phase. The listener object lives on the connection's
//! task and is lent to the game thread for every packet and every 50 ms tick (`on_server`), because tasks, the
//! player list and the spawn preparation need `&mut dyn ServerHandle` — exactly where vanilla runs them
//! (`ensureRunningOnSameThread` + `Connection.tick`).

use super::common_listener::{
    CommonListenerCookie, KeepAliveAction, ServerCommonListener, disconnect_unexpected_query, timeout_disconnection_message,
};
use super::configuration_task::{ConfigurationTask, TaskType};
use super::config::{
    JoinWorldTask, PrepareSpawnTask, ServerCodeOfConductConfigurationTask, ServerResourcePackConfigurationTask, SynchronizeRegistriesTask,
};
use super::connection::{ConnectionSender, ReadError, get_millis};
use super::connection_task::{ConnectionContext, Event, ListenerShared};
use super::hooks::{ConfigurationData, GameThread, NetworkHooks, ServerHandle, on_server};
use super::pending_player::{InboundPacket, PendingPlayer};
use super::play_connection;
use mc_protocol::codec::GameProfile;
use mc_protocol::net::NetError;
use mc_protocol::packet::ConnectionState;
use mc_protocol::packets::common::{ClientInformation, CustomPayload, ResourcePackAction};
use mc_protocol::packets::configuration::{
    ClientboundConfigurationPacket, ClientboundCustomPayloadPacket, ClientboundKeepAlivePacket, ClientboundServerLinksPacket,
    ClientboundUpdateEnabledFeaturesPacket, ServerboundConfigurationPacket,
};
use mc_text::Component;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

fn disconnect_reason_invalid_data() -> Component {
    Component::translatable("multiplayer.disconnect.invalid_player_data", vec![])
}

fn disconnect_reason_configuration_error() -> Component {
    Component::translatable("multiplayer.disconnect.configuration_error", vec![])
}

/// `PlayerList.DUPLICATE_LOGIN_DISCONNECT_MESSAGE`.
fn duplicate_login_disconnect_message() -> Component {
    Component::translatable("multiplayer.disconnect.duplicate_login", vec![])
}

fn illegal_state(message: impl std::fmt::Display) -> String {
    format!("java.lang.IllegalStateException: {message}")
}

/// What handling a packet / tick means for the connection task.
pub(super) enum Flow {
    Continue,
    /// `handleConfigurationFinished` placed the player: the reader switches to the play protocol.
    EnterPlay,
}

pub struct ServerConfigurationPacketListenerImpl {
    common: ServerCommonListener,
    sender: ConnectionSender,
    hooks: Arc<dyn NetworkHooks>,
    game_profile: GameProfile,
    configuration: Arc<ConfigurationData>,
    configuration_tasks: VecDeque<Box<dyn ConfigurationTask>>,
    current_task: Option<Box<dyn ConfigurationTask>>,
    client_information: ClientInformation,
    /// The language the code-of-conduct task reads when it starts (`clientInformation.language()`).
    language: Arc<Mutex<String>>,
    synchronize_registries_task: Option<SynchronizeRegistriesTask>,
    prepare_spawn_task: Option<PrepareSpawnTask>,
    inbound_tx: Sender<InboundPacket>,
    inbound_rx: Option<Receiver<InboundPacket>>,
    transferred: bool,
    listener: Arc<ListenerShared>,
}

impl ServerConfigurationPacketListenerImpl {
    pub(super) fn new(sender: ConnectionSender, hooks: Arc<dyn NetworkHooks>, listener: Arc<ListenerShared>, cookie: CommonListenerCookie) -> Self {
        let (inbound_tx, inbound_rx) = channel();
        Self {
            common: ServerCommonListener::new(&cookie, get_millis()),
            sender,
            configuration: hooks.configuration(),
            hooks,
            game_profile: cookie.profile.clone(),
            configuration_tasks: VecDeque::new(),
            current_task: None,
            language: Arc::new(Mutex::new(cookie.client_information.language.clone())),
            client_information: cookie.client_information,
            synchronize_registries_task: None,
            prepare_spawn_task: None,
            inbound_tx,
            inbound_rx: Some(inbound_rx),
            transferred: cookie.transferred,
            listener,
        }
    }

    /// `ServerConfigurationPacketListenerImpl.send` (a terminal packet closes the common listener).
    fn send(&mut self, packet: ClientboundConfigurationPacket) {
        send_packet(&self.sender, &mut self.common, packet);
    }

    fn is_accepting_messages(&self) -> bool {
        self.sender.is_connected()
    }

    fn disconnect(&self, reason: Component) {
        self.sender.disconnect(reason);
    }

    /// `startConfiguration`.
    pub(super) fn start_configuration(&mut self, server: &mut dyn ServerHandle) -> Result<(), String> {
        let brand = self.hooks.settings().server_mod_name;
        // `BrandPayload.write` = `writeUtf(brand)`: a brand over 32767 chars fails when the packet is encoded.
        match CustomPayload::brand(&brand) {
            Ok(payload) => self.send(ClientboundConfigurationPacket::CustomPayload(ClientboundCustomPayloadPacket { payload })),
            Err(cause) => self.sender.encode_failed(&mc_protocol::packet::PacketEncodeError {
                packet_type: "clientbound/minecraft:custom_payload".to_owned(),
                skippable: false,
                cause,
            }),
        }
        let data = self.configuration.clone();
        if !data.server_links.is_empty() {
            self.send(ClientboundConfigurationPacket::ServerLinks(ClientboundServerLinksPacket { links: data.server_links.clone() }));
        }
        self.send(ClientboundConfigurationPacket::UpdateEnabledFeatures(ClientboundUpdateEnabledFeaturesPacket {
            features: data.enabled_features.clone(),
        }));
        let task = SynchronizeRegistriesTask::new(data.known_packs.clone(), data.registries.clone());
        self.synchronize_registries_task = Some(task.clone());
        self.configuration_tasks.push_back(Box::new(task));
        self.add_optional_tasks();
        self.return_to_world(server)
    }

    /// `returnToWorld`.
    fn return_to_world(&mut self, server: &mut dyn ServerHandle) -> Result<(), String> {
        let task = PrepareSpawnTask::new(self.hooks.prepare_spawn(server, &self.game_profile));
        self.prepare_spawn_task = Some(task.clone());
        self.configuration_tasks.push_back(Box::new(task));
        self.configuration_tasks.push_back(Box::new(JoinWorldTask));
        self.start_next_task(server)
    }

    /// `addOptionalTasks`.
    fn add_optional_tasks(&mut self) {
        let codes = self.configuration.code_of_conducts.clone();
        if !codes.is_empty() {
            let language = self.language.clone();
            self.configuration_tasks.push_back(Box::new(ServerCodeOfConductConfigurationTask::new(move || {
                let language = language.lock().to_lowercase();
                codes
                    .get(&language)
                    .or_else(|| codes.get("en_us"))
                    .or_else(|| codes.values().next())
                    .cloned()
                    .unwrap_or_default()
            })));
        }
        if let Some(info) = self.configuration.resource_pack.clone() {
            self.configuration_tasks.push_back(Box::new(ServerResourcePackConfigurationTask::new(info)));
        }
    }

    /// `startNextTask`.
    fn start_next_task(&mut self, server: &mut dyn ServerHandle) -> Result<(), String> {
        if let Some(current) = &self.current_task {
            return Err(illegal_state(format!("Task {} has not finished yet", current.task_type().id())));
        }
        if self.is_accepting_messages()
            && let Some(mut task) = self.configuration_tasks.pop_front()
        {
            let result = {
                let (sender, common) = (&self.sender, &mut self.common);
                task.start(server, &mut |packet| send_packet(sender, common, packet))
            };
            let task_type = task.task_type();
            self.current_task = Some(task);
            if let Err(e) = result {
                log::error!("Failed to start configuration task {task_type}: {e}");
                self.disconnect(disconnect_reason_configuration_error());
            }
        }
        Ok(())
    }

    /// `finishCurrentTask`.
    fn finish_current_task(&mut self, server: &mut dyn ServerHandle, to_finish: TaskType) -> Result<(), String> {
        let current = self.current_task.as_ref().map(|t| t.task_type());
        if current != Some(to_finish) {
            let current = current.map_or_else(|| "null".to_owned(), |t| t.to_string());
            return Err(illegal_state(format!("Unexpected request for task finish, current task: {current}, requested: {to_finish}")));
        }
        self.current_task = None;
        self.start_next_task(server)
    }

    /// `tick`.
    pub(super) fn tick(&mut self, server: &mut dyn ServerHandle) -> Result<(), String> {
        match self.common.poll_keep_alive(get_millis()) {
            KeepAliveAction::Nothing => {}
            KeepAliveAction::Send(id) => self.send(ClientboundConfigurationPacket::KeepAlive(ClientboundKeepAlivePacket { id })),
            KeepAliveAction::Timeout => self.disconnect(timeout_disconnection_message()),
        }
        if let Some(task) = self.current_task.as_mut() {
            let task_type = task.task_type();
            match task.tick(server) {
                Ok(true) => self.finish_current_task(server, task_type)?,
                Ok(false) => {}
                Err(e) => {
                    log::error!("Failed to tick configuration task {task_type}: {e}");
                    self.disconnect(disconnect_reason_configuration_error());
                }
            }
        }
        if let Some(task) = &self.prepare_spawn_task {
            task.keep_alive(server);
        }
        Ok(())
    }

    /// Dispatch of the serverbound configuration packets. `Err` carries the Java exception text for `exceptionCaught`.
    pub(super) fn handle_packet(&mut self, server: &mut dyn ServerHandle, packet: ServerboundConfigurationPacket) -> Result<Flow, String> {
        match packet {
            ServerboundConfigurationPacket::ClientInformation(p) => {
                *self.language.lock() = p.information.language.clone();
                self.client_information = p.information;
            }
            ServerboundConfigurationPacket::CookieResponse(_) => self.disconnect(disconnect_unexpected_query()),
            ServerboundConfigurationPacket::CustomPayload(_) | ServerboundConfigurationPacket::Pong(_) => {}
            ServerboundConfigurationPacket::CustomClickAction(p) => {
                // `MinecraftServer.handleCustomClickAction`
                let payload = p.payload.0.as_ref().map_or_else(|| "null".to_owned(), |t| t.to_string());
                log::debug!("Received custom click action {} with payload {payload}", p.id);
            }
            ServerboundConfigurationPacket::KeepAlive(p) => {
                if self.common.handle_keep_alive(p.id, get_millis()) {
                    self.disconnect(timeout_disconnection_message());
                }
            }
            ServerboundConfigurationPacket::ResourcePack(p) => {
                if p.action == ResourcePackAction::Declined && self.configuration.is_resource_pack_required() {
                    log::info!("Disconnecting {} due to resource pack {} rejection", self.game_profile.name, p.id);
                    self.disconnect(Component::translatable("multiplayer.requiredTexturePrompt.disconnect", vec![]));
                }
                if p.action.is_terminal() {
                    self.finish_current_task(server, ServerResourcePackConfigurationTask::TYPE)?;
                }
            }
            ServerboundConfigurationPacket::SelectKnownPacks(p) => {
                let Some(task) = self.synchronize_registries_task.clone() else {
                    return Err(illegal_state("Unexpected response from client: received pack selection, but no negotiation ongoing"));
                };
                let (sender, common) = (&self.sender, &mut self.common);
                task.handle_response(&p.known_packs, &mut |packet| send_packet(sender, common, packet));
                self.finish_current_task(server, SynchronizeRegistriesTask::TYPE)?;
            }
            ServerboundConfigurationPacket::AcceptCodeOfConduct(_) => {
                self.finish_current_task(server, ServerCodeOfConductConfigurationTask::TYPE)?;
            }
            ServerboundConfigurationPacket::FinishConfiguration(_) => return self.handle_configuration_finished(server),
        }
        Ok(Flow::Continue)
    }

    /// `handleConfigurationFinished`.
    fn handle_configuration_finished(&mut self, server: &mut dyn ServerHandle) -> Result<Flow, String> {
        self.finish_current_task(server, JoinWorldTask::TYPE)?;
        self.sender.set_outbound_state(ConnectionState::Play);
        let id = self.game_profile.id;
        if self.hooks.is_player_in_world(server, id) {
            self.disconnect(duplicate_login_disconnect_message());
            return Ok(Flow::Continue);
        }
        if let Some(addr) = self.sender.remote_address()
            && let Some(error) = self.hooks.can_player_login(server, addr, &self.game_profile)
        {
            self.disconnect(error);
            return Ok(Flow::Continue);
        }
        let (Some(task), Some(inbound)) = (self.prepare_spawn_task.clone(), self.inbound_rx.take()) else {
            return Err(illegal_state("Player spawn was not ready"));
        };
        let pending = PendingPlayer {
            cookie: CommonListenerCookie {
                profile: self.game_profile.clone(),
                latency: self.common.latency(),
                client_information: self.client_information.clone(),
                transferred: self.transferred,
            },
            sender: self.sender.clone(),
            inbound,
        };
        match task.spawn_player(server, pending) {
            Ok(()) => {
                self.listener.register_play_connection(id, self.sender.handle());
                Ok(Flow::EnterPlay)
            }
            Err(e) => {
                log::error!("Couldn't place player in world: {e}");
                self.disconnect(disconnect_reason_invalid_data());
                Ok(Flow::Continue)
            }
        }
    }

    /// `onDisconnect`.
    fn on_disconnect(&mut self, server: &mut dyn ServerHandle, reason: &str) {
        log::info!("{} ({}) lost connection: {}", self.game_profile.name, self.game_profile.id, reason);
        if let Some(task) = self.prepare_spawn_task.take() {
            task.close(server);
        }
    }
}

/// `ServerCommonPacketListenerImpl.send`.
fn send_packet(sender: &ConnectionSender, common: &mut ServerCommonListener, packet: ClientboundConfigurationPacket) {
    if matches!(packet, ClientboundConfigurationPacket::FinishConfiguration(_)) {
        common.close(get_millis());
    }
    let mut buf = Vec::with_capacity(64);
    match packet.encode_payload(&mut buf) {
        Ok(()) => sender.send_payload(buf),
        // Only reachable for a body the codec refuses; `ConnectionSender::send` handles the same failure for a typed packet.
        Err(cause) => sender.encode_failed(&mc_protocol::packet::PacketEncodeError {
            packet_type: format!("clientbound/{}", packet.name()),
            skippable: false,
            cause,
        }),
    }
}

/// Runs the configuration phase on the connection's task until the connection ends or the player is placed (then the
/// reader turns into the play-protocol inbound pump).
pub(super) async fn run(ctx: &mut ConnectionContext, cookie: CommonListenerCookie) -> Option<CommonListenerCookie> {
    let listener = ServerConfigurationPacketListenerImpl::new(ctx.sender.clone(), ctx.hooks.clone(), ctx.listener.clone(), cookie);
    let game = ctx.game.clone();
    let mut listener = Some(Box::new(listener));

    // `handleLoginAcknowledgement`: `startConfiguration()`.
    let started = lend(&*game, &mut listener, |l, server| l.start_configuration(server)).await?;
    if let Err(cause) = started {
        ctx.fault_cause(&cause);
    }

    loop {
        match ctx.next_event().await {
            Event::Stopped => {
                ctx.wait_closed().await;
                let reason = ctx
                    .handle
                    .disconnection_details()
                    .map_or_else(|| Component::translatable("multiplayer.disconnect.generic", vec![]), |d| d.reason)
                    .get_string();
                // The listener's logging + spawn-task cleanup happen on the server thread, like `handleDisconnection`.
                let _ = lend(&*game, &mut listener, move |l, server| l.on_disconnect(server, &reason)).await;
                return None;
            }
            Event::ReadError(e) => ctx.fault(e),
            Event::Tick => {
                match lend(&*game, &mut listener, |l, server| l.tick(server)).await {
                    None => return None,
                    Some(Err(cause)) => ctx.fault_cause(&cause),
                    Some(Ok(())) => {}
                }
            }
            Event::Payload(p) => match ServerboundConfigurationPacket::decode_payload(&p) {
                Err(e) => ctx.fault(ReadError::Net(NetError::Decode(e))),
                Ok(packet) => {
                    let result = lend(&*game, &mut listener, move |l, server| l.handle_packet(server, packet)).await?;
                    match result {
                        Err(cause) => ctx.fault_cause(&cause),
                        Ok(Flow::Continue) => {}
                        Ok(Flow::EnterPlay) => {
                            let tx = listener.take().expect("listener present").inbound_tx;
                            return play_connection::run(ctx, tx).await;
                        }
                    }
                }
            },
        }
    }
}

/// Moves the listener to the game thread for one call and back. `None` if the server is gone.
async fn lend<R: Send + 'static>(
    game: &dyn GameThread,
    slot: &mut Option<Box<ServerConfigurationPacketListenerImpl>>,
    f: impl FnOnce(&mut ServerConfigurationPacketListenerImpl, &mut dyn ServerHandle) -> R + Send + 'static,
) -> Option<R> {
    let mut listener = slot.take()?;
    let (listener, result) = on_server(game, move |server| {
        let result = f(&mut listener, server);
        (listener, result)
    })
    .await?;
    *slot = Some(listener);
    Some(result)
}
