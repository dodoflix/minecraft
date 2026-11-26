//! The per-connection task: everything netty's event loop does for one `Connection` before the play protocol takes
//! over — legacy-ping sniffing, then the handshake → status | login → configuration listeners (each a module of this
//! directory) run one after the other on the connection's task, with the "tick" that vanilla gets from
//! `ServerConnectionListener.tick` (the server's tick, or every 50 ms for a listener nothing ticks), the 30 s read
//! timeout, packet-rate limiting and `exceptionCaught` handling.

use super::common_listener::CommonListenerCookie;
use super::configuration_listener;
use super::connection::{
    ConnectionHandle, ConnectionSender, FrameReader, ReadError, generic_reason,
    internal_exception_text, run_writer,
};
use super::handshake_listener::{self, HandshakeOutcome};
use super::hooks::{GameThread, NetworkHooks};
use super::legacy_query;
use super::login_listener;
use super::status_listener;
use futures_util::FutureExt;
use mc_core::math::java::java_float_to_string;
use mc_protocol::net::NetError;
use mc_protocol::packet::ConnectionState;
use mc_protocol::packets::common::ClientInformation;
use mc_protocol::packets::handshake::ServerboundHandshakePacket;
use mc_text::Component;
use parking_lot::Mutex;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{Interval, MissedTickBehavior, interval};
use uuid::Uuid;

/// State of the listener that all connections share (`ServerConnectionListener.sessionId`, the connection list).
pub struct ListenerShared {
    session_id: Mutex<Option<Uuid>>,
    connections: Mutex<Vec<Arc<ConnectionHandle>>>,
    /// Connections whose listener lives on the game thread (play protocol): ticked by `ServerConnectionListener::tick`.
    play_connections: Mutex<Vec<(Uuid, Arc<ConnectionHandle>)>>,
    /// The number of server ticks so far: what ticks the connections of a listener the server ticks.
    server_ticks: watch::Sender<u64>,
    /// Set once the server ticks this listener (`ServerConnectionListener.tick`): connections then tick with the server
    /// instead of by the clock.
    ticked_by_server: std::sync::atomic::AtomicBool,
}

impl ListenerShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            session_id: Mutex::new(None),
            connections: Mutex::new(Vec::new()),
            play_connections: Mutex::new(Vec::new()),
            server_ticks: watch::channel(0).0,
            ticked_by_server: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// One server tick of `ServerConnectionListener.tick`: every login and configuration connection ticks.
    pub(super) fn signal_server_tick(&self) {
        self.ticked_by_server
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.server_ticks.send_modify(|ticks| *ticks += 1);
    }

    /// `getSessionId`: one random UUID for as long as there are connections.
    pub fn session_id(&self) -> Uuid {
        *self.session_id.lock().get_or_insert_with(Uuid::new_v4)
    }

    pub(super) fn add_connection(&self, handle: Arc<ConnectionHandle>) {
        self.connections.lock().push(handle);
    }

    /// Drops closed connections; clears the session id when none is left (`ServerConnectionListener.tick`).
    pub(super) fn prune(&self) {
        let mut connections = self.connections.lock();
        connections.retain(|c| c.is_connected());
        if connections.is_empty() {
            *self.session_id.lock() = None;
        }
    }

    /// A connection entered the play protocol for player `id` (`Connection.tick` now drives its play listener).
    pub(super) fn register_play_connection(&self, id: Uuid, handle: &Arc<ConnectionHandle>) {
        let mut play = self.play_connections.lock();
        play.retain(|(_, h)| h.id() != handle.id());
        play.push((id, handle.clone()));
    }

    /// The play connections to tick, in connection order, each with whether it was already closed at this point.
    pub(super) fn play_snapshot(&self) -> Vec<(Uuid, bool)> {
        self.play_connections
            .lock()
            .iter()
            .map(|(id, h)| (*id, !h.is_connected()))
            .collect()
    }

    /// Forgets the play connections that were closed when `snapshot` was taken (their listener saw the disconnect).
    pub(super) fn drop_closed_play_connections(&self, snapshot: &[(Uuid, bool)]) {
        self.play_connections.lock().retain(|(id, h)| {
            !(snapshot.iter().any(|(i, closed)| i == id && *closed) && !h.is_connected())
        });
    }

    pub fn connection_count(&self) -> usize {
        self.connections.lock().len()
    }
}

/// What a connection task waits for.
pub(super) enum Event {
    /// A packet payload (`VarInt id + body`) arrived.
    Payload(Vec<u8>),
    /// One game tick (50 ms) passed (`Connection.tick`).
    Tick,
    /// The connection was closed or turned read-only (a disconnect is under way).
    Stopped,
    /// Reading/decoding failed (`exceptionCaught`): call [`ConnectionContext::fault`].
    ReadError(ReadError),
}

/// The ticks of the server a connection follows.
struct ServerTicks {
    receiver: watch::Receiver<u64>,
    /// The tick counter last read from `receiver`.
    seen: u64,
    /// Ticks read but not yet delivered.
    pending: u64,
    /// `Connection.tickCount`.
    tick_count: u64,
}

/// Waits for the next server tick; `true` when a tick is due (never when the connection runs by the clock).
async fn wait_server_tick(ticks: &mut Option<ServerTicks>) -> bool {
    let Some(ticks) = ticks else {
        return std::future::pending().await;
    };
    if ticks.pending == 0 {
        if ticks.receiver.changed().await.is_err() {
            return std::future::pending().await;
        }
        let now = *ticks.receiver.borrow_and_update();
        ticks.pending = now - ticks.seen;
        ticks.seen = now;
    }
    true
}

/// A fault is a failure of the pipeline (not a clean disconnect decision of a listener).
pub(super) struct ConnectionContext {
    pub reader: FrameReader,
    pub sender: ConnectionSender,
    pub handle: Arc<ConnectionHandle>,
    pub hooks: Arc<dyn NetworkHooks>,
    pub game: Arc<dyn GameThread>,
    pub listener: Arc<ListenerShared>,
    tick: Interval,
    second: Interval,
    /// The server ticks still to be delivered as [`Event::Tick`] (`Some`: the connection ticks with the server).
    server_ticks: Option<ServerTicks>,
    /// `RateKickingConnection` limit (`<= 0` = off).
    rate_limit: i32,
    received_packets: u32,
    average_received_packets: f32,
    handling_fault: bool,
}

impl ConnectionContext {
    /// `Connection.tick`'s once-a-second part (`tickSecond`) + the rate-limit kick.
    fn tick_second(&mut self) {
        // Mth.lerp(0.75F, received, average)
        self.average_received_packets = self.received_packets as f32
            + 0.75 * (self.average_received_packets - self.received_packets as f32);
        self.received_packets = 0;
        if self.rate_limit > 0 && self.average_received_packets > self.rate_limit as f32 {
            let average = self.average_received_packets;
            self.log_on_server(move || {
                log::warn!(
                    "Player exceeded rate-limit (sent {} packets per second)",
                    java_float_to_string(average)
                )
            });
            self.sender.disconnect(Component::translatable(
                "disconnect.exceeded_packet_rate",
                vec![],
            ));
        }
    }

    /// `Connection.tick` on the server's tick: the once-a-second part every 20th tick.
    fn deliver_server_tick(&mut self) {
        let Some(ticks) = &mut self.server_ticks else {
            return;
        };
        ticks.pending -= 1;
        let count = ticks.tick_count;
        ticks.tick_count += 1;
        if count % 20 == 0 {
            self.tick_second();
        }
    }

    /// Next thing the connection task has to react to.
    pub(super) async fn next_event(&mut self) -> Event {
        loop {
            tokio::select! {
                biased;
                () = self.handle.stopped() => return Event::Stopped,
                _ = self.second.tick(), if self.server_ticks.is_none() => self.tick_second(),
                _ = self.tick.tick(), if self.server_ticks.is_none() => return Event::Tick,
                due = wait_server_tick(&mut self.server_ticks) => {
                    if due {
                        self.deliver_server_tick();
                        return Event::Tick;
                    }
                }
                r = self.reader.next_payload() => {
                    return match r {
                        Ok(p) => {
                            self.received_packets = self.received_packets.wrapping_add(1);
                            self.handle.record_received_packet(&p);
                            Event::Payload(p)
                        }
                        Err(e) => Event::ReadError(e),
                    };
                }
            }
        }
    }

    /// `Connection.exceptionCaught` for a read failure.
    pub(super) fn fault(&mut self, error: ReadError) {
        match error {
            ReadError::Eof => {
                // `channelInactive`: the channel is already closed, so `disconnect(disconnect.endOfStream)` records no
                // reason and `handleDisconnection` reports `multiplayer.disconnect.generic` ("Disconnected").
                self.handle.set_read_only();
                self.sender.close();
            }
            ReadError::Timeout => {
                log::debug!("Timeout");
                self.sender
                    .close_with(Component::translatable("disconnect.timeout", vec![]));
            }
            ReadError::Net(e) => self.fault_text(&internal_exception_text(&e)),
        }
    }

    /// `exceptionCaught` with an `Internal Exception: <cause>` text (`cause` is `Throwable.toString()`).
    pub(super) fn fault_cause(&mut self, cause: &str) {
        self.fault_text(&format!("Internal Exception: {cause}"));
    }

    fn fault_text(&mut self, text: &str) {
        let reason = generic_reason(text.to_owned());
        if !self.handle.is_connected() {
            return;
        }
        if !self.handling_fault {
            self.handling_fault = true;
            log::debug!("Failed to sent packet: {text}");
            // First fault: tell the client (disconnect packet of the outbound protocol), stop reading.
            self.sender.disconnect(reason);
        } else {
            log::debug!("Double fault: {text}");
            self.sender.close_with(reason);
        }
    }

    /// Runs `f` on the game thread; vanilla logs from the server thread what is emitted from `tick` /
    /// `handleDisconnection` there.
    pub(super) fn log_on_server(&self, f: impl FnOnce() + Send + 'static) {
        self.game.execute(Box::new(move |_| f()));
    }

    /// Waits until the writer task has closed the socket (after a disconnect packet is flushed), at most 5 s.
    pub(super) async fn wait_closed(&self) {
        let _ = tokio::time::timeout(Duration::from_secs(5), self.handle.closed()).await;
    }
}

/// Creates the tasks of an accepted socket and runs the connection until it ends or the play protocol takes over.
pub(super) async fn run_connection(
    socket: TcpStream,
    id: u64,
    hooks: Arc<dyn NetworkHooks>,
    listener: Arc<ListenerShared>,
) {
    let _ = socket.set_nodelay(true);
    let remote = socket.peer_addr().ok();
    let (read, write) = socket.into_split();
    let handle = ConnectionHandle::new(id, remote);
    let (tx, rx) = mpsc::unbounded_channel();
    let sender = ConnectionSender::with_handle(tx, handle.clone());
    tokio::spawn(run_writer(write, rx, handle.clone()));
    listener.add_connection(handle.clone());

    let settings = hooks.settings();
    let receiver = listener.server_ticks.subscribe();
    let seen = *receiver.borrow();
    let ticked = listener
        .ticked_by_server
        .load(std::sync::atomic::Ordering::SeqCst);
    let mut tick = interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut second = interval(Duration::from_secs(1));
    second.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut ctx = ConnectionContext {
        reader: FrameReader::new(read),
        sender,
        handle,
        game: hooks.game_thread(),
        hooks,
        listener,
        tick,
        second,
        server_ticks: ticked.then(|| ServerTicks {
            seen,
            receiver,
            pending: 0,
            tick_count: 0,
        }),
        rate_limit: settings.rate_limit_packets_per_second,
        received_packets: 0,
        average_received_packets: 0.0,
        handling_fault: false,
    };

    if settings.replies_to_status {
        match legacy_query::sniff(&mut ctx).await {
            legacy_query::Sniffed::Answered => return,
            legacy_query::Sniffed::Normal => {}
        }
    }
    // A panic in any phase (handshake, status, login, configuration) closes only this connection.
    if let Err(panic) = AssertUnwindSafe(run_protocols(&mut ctx))
        .catch_unwind()
        .await
    {
        let message = crate::crash_report::panic_message(panic.as_ref());
        log::error!("Failed to handle a packet of connection {id}: {message}");
        ctx.fault_cause(&message);
    }
    ctx.wait_closed().await;
}

async fn run_protocols(ctx: &mut ConnectionContext) {
    // Handshake
    let intention = loop {
        match ctx.next_event().await {
            Event::Tick => {}
            Event::Stopped => return,
            Event::ReadError(e) => ctx.fault(e),
            Event::Payload(p) => match ServerboundHandshakePacket::decode_payload(&p) {
                Ok(ServerboundHandshakePacket::ClientIntention(packet)) => break packet,
                Err(e) => ctx.fault(ReadError::Net(NetError::Decode(e))),
            },
        }
    };
    match handshake_listener::handle_intention(ctx, intention).await {
        HandshakeOutcome::Closed => {}
        HandshakeOutcome::Status(status) => status_listener::run(ctx, status).await,
        HandshakeOutcome::Login { transferred } => {
            if let Some(profile) = login_listener::run(ctx).await {
                ctx.sender
                    .set_outbound_state(ConnectionState::Configuration);
                let mut cookie = CommonListenerCookie {
                    profile,
                    latency: 0,
                    client_information: ClientInformation::default(),
                    transferred,
                };
                // Play -> configuration switches (`ServerboundConfigurationAcknowledgedPacket`) come back here.
                while let Some(next) = configuration_listener::run(ctx, cookie).await {
                    cookie = next;
                }
            }
        }
    }
}
