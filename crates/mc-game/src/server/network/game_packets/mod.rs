//! Behavior of `ServerGamePacketListenerImpl` (and the play half of `ServerCommonPacketListenerImpl`): the per-tick
//! driver, the inbound dispatcher and one file per concern. Every handler is a method of [`PlayCtx`], named like
//! vanilla's (`handle_move_player`, `handle_chat`, ...), reading/writing the listener state through
//! [`PlayCtx::lis`].
//!
//! # Threading model of the inbound queue
//! Vanilla handles some packets directly on the netty thread (keep-alive, chat family, command suggestions, ping,
//! book/sign editing, configuration acknowledgement; chat then queues a server task) and the rest on the main thread
//! through the `PacketProcessor`, which is drained once at the very start of every tick (before `tickServer`).
//! The queued tasks of the netty-handled packets run before that drain. [`drain_inbound`] reproduces this
//! ordering for the batch of packets that arrived since the previous tick: first the netty-thread packets in arrival
//! order, then the main-thread ones in arrival order.
//!
//! # Extension points for later waves
//! Packets whose subsystem is not part of the listener (containers, recipe book, advancements, command blocks,
//! structure blocks, signs, books, pick item, tag queries, ...) call into [`hooks::PlayHandlers`] after the listener
//! level checks vanilla does itself.

mod chat;
mod commands_host;
mod interaction;
mod movement;
pub(crate) mod seam;
mod services_keys;
mod session;
mod technical_blocks;

pub mod hooks;

use super::game_listener::ServerGamePacketListenerImpl;
use super::InboundPacket;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::ServerboundPlayPacket;
use mc_text::Component;
use uuid::Uuid;

/// The transient borrow in which a packet or a tick of one player is handled: vanilla's `this.server`,
/// `this.player.level()` and `this.player`. The player is taken out of the level's entity storage for the duration
/// (queries for it return `None`, like vanilla queries that exclude `this`).
pub struct PlayCtx<'a> {
    pub server: &'a mut MinecraftServer,
    pub level: &'a mut ServerLevel,
    pub player: &'a mut ServerPlayer,
}

impl PlayCtx<'_> {
    /// The listener (`player.connection`). A context only exists for players that have one.
    pub fn lis(&mut self) -> &mut ServerGamePacketListenerImpl {
        self.player.listener.as_mut().expect("play context without a listener")
    }

    /// `send(packet)`.
    pub fn send<P: Packet>(&mut self, packet: &P) {
        self.lis().send(packet);
    }

    /// `disconnect(reason)`.
    pub fn disconnect(&mut self, reason: Component) {
        self.lis().disconnect(reason);
    }

    /// `hasClientLoaded()`.
    pub fn has_client_loaded(&mut self) -> bool {
        self.lis().has_client_loaded()
    }

    /// `Util.getMillis()`.
    pub fn now_ms(&self) -> i64 {
        crate::util::util::get_millis()
    }

    /// `server.getPlayerList().isOp(player.nameAndId())`.
    pub(super) fn player_is_op(&self) -> bool {
        self.server.player_list.is_op(&self.player.name_and_id())
    }
}

/// Packets vanilla handles on the netty thread (no `ensureRunningOnSameThread`).
fn handled_on_netty_thread(packet: &ServerboundPlayPacket) -> bool {
    use ServerboundPlayPacket as P;
    matches!(
        packet,
        P::KeepAlive(_)
            | P::Pong(_)
            | P::CustomPayload(_)
            | P::CookieResponse(_)
            | P::PingRequest(_)
            | P::ChatAck(_)
            | P::Chat(_)
            | P::ChatCommand(_)
            | P::ChatCommandSigned(_)
            | P::CommandSuggestion(_)
            | P::EditBook(_)
            | P::SignUpdate(_)
            | P::ConfigurationAcknowledged(_)
    )
}

/// Runs `f` with the context of the player `uuid`: finds the player's level, takes the player out of it
/// (`ServerLevel::with_player`) and puts it back afterwards. Returns `None` if the player or its level is not
/// available (not in the player list, level currently ticking).
pub fn with_player<R>(server: &mut MinecraftServer, uuid: Uuid, f: impl FnOnce(&mut PlayCtx) -> R) -> Option<R> {
    let (entity_id, dimension) = {
        let handle = server.player_list.get_player(&uuid)?;
        (handle.id, handle.dimension())
    };
    server
        .with_level(&dimension, |server, level| level.with_player(entity_id, |level, player| f(&mut PlayCtx { server, level, player })))
        .flatten()
}

/// `PacketProcessor.processQueuedPackets()` for every connected player; called at the start of each tick.
pub fn process_queued_packets(server: &mut MinecraftServer) {
    let uuids: Vec<Uuid> = server.player_list.players().iter().map(|p| p.uuid).collect();
    for uuid in uuids {
        let Some(handle) = server.player_list.get_player(&uuid).cloned() else { continue };
        let Some((pending, deferred)) = with_player(server, uuid, |ctx| {
            ctx.drain_inbound();
            (ctx.lis().pending_respawn.take(), std::mem::take(&mut ctx.lis().deferred_tasks))
        }) else {
            continue;
        };
        for task in deferred {
            task(server);
        }
        if let Some(won_game) = pending {
            perform_respawn(server, &handle, won_game);
        }
    }
}

/// `MinecraftServer.tickChildren` "send chunks": every player's `chunkSender.sendNextChunks(player)`.
pub fn send_next_chunks_all(server: &mut MinecraftServer) {
    let uuids: Vec<Uuid> = server.player_list.players().iter().map(|p| p.uuid).collect();
    for uuid in uuids {
        with_player(server, uuid, |ctx| {
            let pos = crate::world::entity::Entity::chunk_position(&*ctx.player);
            let sender = ctx.lis().chunk_sender.clone();
            sender.send_next_chunks(pos, &mut super::player_chunk_sender::LevelChunkSource { level: &*ctx.level });
        });
    }
}

/// `Connection.tick()` for one play connection (called by `ServerConnectionListener.tick`): the listener's tick,
/// then `handleDisconnection` once the connection is closed.
pub fn tick_connection(server: &mut MinecraftServer, uuid: Uuid) {
    let found = with_player(server, uuid, |ctx| {
        ctx.tick();
        ctx.handle_disconnection_if_closed();
    });
    if found.is_none() {
        log::warn!("play tick: player {uuid} not found (handle/level/entity lookup failed)");
    }
}

impl PlayCtx<'_> {
    /// Drain this connection's inbound channel (see the module docs for the ordering).
    fn drain_inbound(&mut self) {
        // A player placed by a server task between ticks has not run its first `tick()` yet: vanilla places it while
        // processing packets and ticks the connection in the same server tick, so `firstGood`/`lastGood` already hold
        // the spawn position when the client's first packets (accept-teleport) are handled.
        if self.lis().tick_count == 0 {
            self.reset_position();
        }
        let mut batch = Vec::new();
        let lis = self.lis();
        while let Ok(event) = lis.inbound.try_recv() {
            batch.push(event);
        }
        let mut netty = Vec::new();
        let mut main = Vec::new();
        for event in batch {
            match event {
                InboundPacket::Packet { packet, received_at_ms } if handled_on_netty_thread(&packet) => netty.push((*packet, received_at_ms)),
                InboundPacket::Packet { packet, received_at_ms } => main.push((*packet, received_at_ms)),
                InboundPacket::Disconnected(details) => {
                    lis.pending_disconnect.get_or_insert(details.reason.clone());
                }
            }
        }
        for (packet, received_at_ms) in netty {
            if !self.lis().sender.is_connected() {
                break;
            }
            self.dispatch(packet, received_at_ms);
        }
        for (packet, received_at_ms) in main {
            // `PacketProcessor`: `shouldHandleMessage` = `isAcceptingMessages()` (a closed connection, or one
            // waiting for the switch to configuration, ignores everything but `ConfigurationAcknowledged`).
            let closed_for_good = self.lis().pending_disconnect.is_some();
            if closed_for_good || !self.lis().is_accepting_messages() {
                log::debug!("Ignoring packet due to disconnection: {:?}", packet.name());
                continue;
            }
            self.dispatch(packet, received_at_ms);
        }
    }

    /// `onPacketError`: vanilla rethrows as a `ReportedException` (server crash). We log the error and disconnect the
    /// offending player instead (documented deviation: a bad packet must not take the server down).
    fn on_packet_error(&mut self, packet_name: &str, error: String) {
        log::error!("Failed to handle packet {packet_name}: {error}");
        self.disconnect(Component::translatable("disconnect.genericReason", vec![format!("Internal Exception: {error}").into()]));
    }

    /// `packet.handle(listener)` for every serverbound play packet.
    fn dispatch(&mut self, packet: ServerboundPlayPacket, received_at_ms: i64) {
        use ServerboundPlayPacket as P;
        let name = packet.name();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match packet {
            // common
            P::KeepAlive(p) => self.handle_keep_alive(&p, received_at_ms),
            P::Pong(p) => self.handle_pong(&p),
            P::CustomPayload(p) => self.handle_custom_payload(&p),
            P::CustomClickAction(p) => self.handle_custom_click_action(&p),
            P::ResourcePack(p) => self.handle_resource_pack_response(&p),
            P::CookieResponse(p) => self.handle_cookie_response(&p),
            P::ClientInformation(p) => self.handle_client_information(&p),
            P::PingRequest(p) => self.handle_ping_request(&p),
            // movement
            P::MovePlayerPos(p) => self.handle_move_player(&movement::MovePlayer::from(&p)),
            P::MovePlayerPosRot(p) => self.handle_move_player(&movement::MovePlayer::from(&p)),
            P::MovePlayerRot(p) => self.handle_move_player(&movement::MovePlayer::from(&p)),
            P::MovePlayerStatusOnly(p) => self.handle_move_player(&movement::MovePlayer::from(&p)),
            P::MoveVehicle(p) => self.handle_move_vehicle(&p),
            P::AcceptTeleportation(p) => self.handle_accept_teleport_packet(&p),
            P::PlayerLoaded(p) => self.handle_accept_player_load(&p),
            P::PlayerInput(p) => self.handle_player_input(&p),
            P::PlayerCommand(p) => self.handle_player_command(&p),
            P::PlayerAbilities(p) => self.handle_player_abilities(&p),
            P::ClientTickEnd(p) => self.handle_client_tick_end(&p),
            P::ChunkBatchReceived(p) => self.handle_chunk_batch_received(&p),
            // chat
            P::Chat(p) => self.handle_chat(&p),
            P::ChatCommand(p) => self.handle_chat_command(&p),
            P::ChatCommandSigned(p) => self.handle_signed_chat_command(&p),
            P::ChatAck(p) => self.handle_chat_ack(&p),
            P::ChatSessionUpdate(p) => self.handle_chat_session_update(&p),
            P::CommandSuggestion(p) => self.handle_custom_command_suggestions(&p),
            // interaction
            P::PlayerAction(p) => self.handle_player_action(&p),
            P::UseItemOn(p) => self.handle_use_item_on(&p),
            P::UseItem(p) => self.handle_use_item(&p),
            P::Interact(p) => self.handle_interact(&p),
            P::Attack(p) => self.handle_attack(&p),
            P::Punch(p) => self.handle_punch(&p),
            P::SetCarriedItem(p) => self.handle_set_carried_item(&p),
            P::SpectatorAction(p) => self.handle_spectator_action(&p),
            P::TeleportToEntity(p) => self.handle_teleport_to_entity_packet(&p),
            P::PaddleBoat(p) => self.handle_paddle_boat(&p),
            // session
            P::ClientCommand(p) => self.handle_client_command(&p),
            P::ChangeDifficulty(p) => self.handle_change_difficulty(&p),
            P::ChangeGameMode(p) => self.handle_change_game_mode(&p),
            P::LockDifficulty(p) => self.handle_lock_difficulty(&p),
            P::ConfigurationAcknowledged(p) => self.handle_configuration_acknowledged(&p),
            // later waves (listener-level checks first, then the hook)
            other => self.dispatch_hook(other),
        }));
        if let Err(panic) = result {
            let message = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            self.on_packet_error(name, message);
        }
    }
}

/// Starts fetching the Mojang Services profile-key validators (idempotent; vanilla does it at server start).
pub fn start_services_keys_fetch() {
    services_keys::start_fetch();
}

/// `handleClientCommand` PERFORM_RESPAWN: `this.player = playerList.respawn(this.player, keepAll, reason)` followed by
/// `resetPosition()`, `restartClientLoadTimerAfterRespawn()` and, for a normal respawn on a hardcore world, the switch
/// to spectator mode.
fn perform_respawn(server: &mut MinecraftServer, handle: &crate::server::players::PlayerHandle, won_game: bool) {
    use crate::world::entity::{Entity, RemovalReason};
    let reason = if won_game { RemovalReason::ChangedDimension } else { RemovalReason::Killed };
    let hardcore = server.world_data.lock().settings.difficulty_settings.hardcore;
    crate::server::players::respawn::respawn(server, &handle.dimension(), handle.id, won_game, reason, move |_server, level, player| {
        let pos = player.position();
        if let Some(lis) = player.listener.as_mut() {
            lis.reset_position(pos);
            lis.restart_client_load_timer_after_respawn();
        }
        if !won_game && hardcore {
            player.set_game_mode(level, mc_data::enums::GameType::Spectator);
        }
    });
}
