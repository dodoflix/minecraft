//! `net.minecraft.network.Connection` on tokio: the shared handle of one client connection, the send side usable from
//! any thread (`ConnectionSender`), the writer task (framing, compression, encryption — `mc_protocol::net`), and the
//! reader (`FrameReader`: read timeout, legacy-ping sniffing, frame decoding).
//!
//! Outbound packets are pre-encoded payloads (`VarInt id + body`) pushed through an unbounded queue; the writer task
//! drains it in batches. State-dependent things (compression, encryption, the outbound protocol used to encode a
//! disconnect packet) are queue messages so they stay ordered with the packets around them, like netty's pipeline
//! reconfiguration tasks.

use super::common_listener::CommonListenerCookie;
use mc_protocol::codec::{Encode, write_varint};
use mc_protocol::net::{FrameDecoder, FrameEncoder, NetError};
use mc_protocol::packet::{ConnectionState, Packet};
use mc_protocol::packets::{configuration, login, play};
use mc_text::Component;
use parking_lot::Mutex;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Notify, mpsc::UnboundedReceiver, mpsc::UnboundedSender};
use tokio::time::Instant;

/// `ReadTimeoutHandler(30)`.
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// `DisconnectionDetails`.
#[derive(Debug, Clone, PartialEq)]
pub struct DisconnectionDetails {
    pub reason: Component,
    pub report: Option<PathBuf>,
    pub bug_report_link: Option<String>,
}

impl DisconnectionDetails {
    pub fn new(reason: Component) -> Self {
        Self { reason, report: None, bug_report_link: None }
    }
}

/// What the connection's writer task is asked to do.
pub enum Outbound {
    /// One packet payload (`VarInt id + body`), shared so a broadcast encodes once. Flushed right away.
    Payload(Arc<[u8]>),
    /// Like `Payload` but may stay buffered until the next flush (`Connection.send(.., flush = false)`); the writer
    /// flushes at the latest 50 ms later (`Connection.tick`'s `channel.flush()`).
    Write(Arc<[u8]>),
    /// `Connection.flushChannel`.
    Flush,
    /// `Connection.setupCompression`, outbound half: a threshold `>= 0` enables compression, negative disables it.
    SetCompression(i32),
    /// `Connection.setEncryptionKey`, outbound half.
    EnableEncryption([u8; 16]),
    /// `Connection.setupOutboundProtocol`: the protocol the disconnect packet below is encoded for.
    SetState(ConnectionState),
    /// The disconnect packet of the current outbound protocol (`ClientboundLoginDisconnect` / `ClientboundDisconnect`;
    /// none for handshake/status), then flush and close the connection.
    Disconnect(Component),
    /// Flush, then close the connection.
    Close,
    /// Bytes written to the socket as they are (no framing/compression/encryption): the legacy server-list reply.
    Raw(Vec<u8>),
}

/// State shared by everything that touches one connection (the part of vanilla's `Connection` that is not the pipeline).
pub struct ConnectionHandle {
    id: u64,
    remote: Option<SocketAddr>,
    /// `!channel.isOpen()`.
    closed: AtomicBool,
    /// `channel.config().setAutoRead(false)` (`Connection.setReadOnly`): the reader stops consuming packets.
    read_only: AtomicBool,
    details: Mutex<Option<DisconnectionDetails>>,
    notify: Notify,
    /// Where the play-protocol reader waits for `switch_to_configuration` (set while the connection is in play).
    reconfigure: Mutex<Option<UnboundedSender<CommonListenerCookie>>>,
}

impl ConnectionHandle {
    pub fn new(id: u64, remote: Option<SocketAddr>) -> Arc<Self> {
        Arc::new(Self {
            id,
            remote,
            closed: AtomicBool::new(false),
            read_only: AtomicBool::new(false),
            details: Mutex::new(None),
            notify: Notify::new(),
            reconfigure: Mutex::new(None),
        })
    }

    /// Unique per listener, in accept order.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// `Connection.getRemoteAddress`; `None` only for connections without a socket (tests).
    pub fn remote_address(&self) -> Option<SocketAddr> {
        self.remote
    }

    /// `Connection.getLoggableAddress`.
    pub fn loggable_address(&self, log_ips: bool) -> String {
        match self.remote {
            None => "local".to_owned(),
            Some(addr) if log_ips => java_socket_address_string(&addr),
            Some(_) => "IP hidden".to_owned(),
        }
    }

    /// `Connection.isConnected`.
    pub fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only.load(Ordering::Acquire)
    }

    /// `Connection.getDisconnectionDetails`.
    pub fn disconnection_details(&self) -> Option<DisconnectionDetails> {
        self.details.lock().clone()
    }

    pub(crate) fn set_reconfigure_sender(&self, tx: UnboundedSender<CommonListenerCookie>) {
        *self.reconfigure.lock() = Some(tx);
    }

    /// `Connection.setReadOnly`.
    pub fn set_read_only(&self) {
        self.read_only.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    /// Records why the connection ends (the first reason wins: once the channel is closed `disconnect` is a no-op).
    pub fn set_disconnection_details(&self, details: DisconnectionDetails) {
        if self.is_connected() {
            self.details.lock().get_or_insert(details);
        }
    }

    pub(crate) fn mark_closed(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    /// Resolves when the connection is closed.
    pub(crate) async fn closed(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !self.is_connected() {
                return;
            }
            notified.await;
        }
    }

    /// Resolves when the connection was closed or made read-only.
    pub(crate) async fn stopped(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // Register before checking the flags so a concurrent notify_waiters is not lost.
            notified.as_mut().enable();
            if self.is_read_only() || !self.is_connected() {
                return;
            }
            notified.await;
        }
    }
}

/// `InetSocketAddress.toString()`: `/1.2.3.4:25565`, IPv6 as `/[0:0:0:0:0:0:0:1]:25565` (Java never compresses zero
/// groups and unwraps IPv4-mapped addresses).
pub fn java_socket_address_string(addr: &SocketAddr) -> String {
    format!("/{}:{}", java_host_address(&addr.ip(), true), addr.port())
}

/// `InetAddress.getHostAddress()` (`bracket`: wrap IPv6 in `[]` like the socket address form).
pub fn java_host_address(ip: &IpAddr, bracket: bool) -> String {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let groups = v6.segments().map(|g| format!("{g:x}")).join(":");
            if bracket { format!("[{groups}]") } else { groups }
        }
    }
}

/// Send side of a client connection, usable from any thread (clone-able). The writer task of the connection owns the
/// socket; everything the game thread sends goes through this handle as pre-encoded payloads (`VarInt id + body`).
#[derive(Clone)]
pub struct ConnectionSender {
    tx: UnboundedSender<Outbound>,
    handle: Arc<ConnectionHandle>,
}

impl ConnectionSender {
    /// A sender without a socket behind it (the receiver side is whoever drains `tx`; tests).
    pub fn new(tx: UnboundedSender<Outbound>) -> Self {
        Self { tx, handle: ConnectionHandle::new(0, None) }
    }

    pub fn with_handle(tx: UnboundedSender<Outbound>, handle: Arc<ConnectionHandle>) -> Self {
        Self { tx, handle }
    }

    pub fn handle(&self) -> &Arc<ConnectionHandle> {
        &self.handle
    }

    /// `Connection.isConnected`.
    pub fn is_connected(&self) -> bool {
        self.handle.is_connected()
    }

    /// `Connection.getRemoteAddress`.
    pub fn remote_address(&self) -> Option<SocketAddr> {
        self.handle.remote_address()
    }

    /// `Connection.send(packet)`: encodes `VarInt id + body` and queues it. A closed connection drops the packet.
    /// A packet whose encoder refuses its data (mc-protocol encoders panic where vanilla throws an
    /// `EncoderException`) disconnects only this connection, like vanilla's `exceptionCaught`.
    pub fn send<P: Packet>(&self, packet: &P) {
        if let Some(payload) = self.encode_guarded(packet) {
            self.send_payload(payload);
        }
    }

    /// `Connection.send(packet, null, flush)`.
    pub fn send_flush<P: Packet>(&self, packet: &P, flush: bool) {
        if let Some(payload) = self.encode_guarded(packet) {
            let _ = self.tx.send(if flush { Outbound::Payload(payload) } else { Outbound::Write(payload) });
        }
    }

    /// Encodes `packet` (`VarInt id + body`); on an encoder failure logs it, kicks this connection with vanilla's
    /// `Internal Exception: io.netty.handler.codec.EncoderException: Failed to encode packet ...` and returns `None`.
    pub fn encode_guarded<P: Packet>(&self, packet: &P) -> Option<Arc<[u8]>> {
        match try_encode_payload(packet) {
            Ok(payload) => Some(payload),
            Err(message) => {
                log::error!("Failed to encode packet 'clientbound/{}': {message}", P::NAME);
                let cause = format!("io.netty.handler.codec.EncoderException: Failed to encode packet 'clientbound/{}'", P::NAME);
                self.disconnect(generic_reason(format!("Internal Exception: {cause}")));
                None
            }
        }
    }

    pub fn send_payload(&self, payload: Arc<[u8]>) {
        let _ = self.tx.send(Outbound::Payload(payload));
    }

    /// `Connection.flushChannel`.
    pub fn flush(&self) {
        let _ = self.tx.send(Outbound::Flush);
    }

    /// `Connection.setupOutboundProtocol` (the part that matters on the wire: which disconnect packet to use).
    pub fn set_outbound_state(&self, state: ConnectionState) {
        let _ = self.tx.send(Outbound::SetState(state));
    }

    /// Outbound half of `Connection.setupCompression`. (The inbound half is applied by the reader owner.)
    pub fn set_compression(&self, threshold: i32) {
        let _ = self.tx.send(Outbound::SetCompression(threshold));
    }

    /// Outbound half of `Connection.setEncryptionKey`.
    pub fn enable_encryption(&self, secret: [u8; 16]) {
        let _ = self.tx.send(Outbound::EnableEncryption(secret));
    }

    /// `send(new ClientboundDisconnectPacket(reason), thenRun(connection.disconnect(details)))` +
    /// `connection.setReadOnly()`: queues the disconnect packet of the current outbound protocol, flushes and closes;
    /// the reader stops consuming packets. The first disconnect reason wins.
    pub fn disconnect(&self, reason: Component) {
        self.disconnect_with(DisconnectionDetails::new(reason));
    }

    pub fn disconnect_with(&self, details: DisconnectionDetails) {
        let reason = details.reason.clone();
        self.handle.set_disconnection_details(details);
        self.handle.set_read_only();
        let _ = self.tx.send(Outbound::Disconnect(reason));
    }

    /// `Connection.disconnect(reason)`: flush what is queued, close without a disconnect packet.
    pub fn close_with(&self, reason: Component) {
        self.handle.set_disconnection_details(DisconnectionDetails::new(reason));
        self.handle.set_read_only();
        let _ = self.tx.send(Outbound::Close);
    }

    pub fn close(&self) {
        let _ = self.tx.send(Outbound::Close);
    }

    /// `ServerGamePacketListenerImpl.handleConfigurationAcknowledged`'s protocol switch: the outbound protocol becomes
    /// `configuration`, and the reader (paused after `ServerboundConfigurationAcknowledgedPacket`) starts a new
    /// configuration listener with `cookie`. `false` if the connection is not in the play protocol.
    pub fn switch_to_configuration(&self, cookie: CommonListenerCookie) -> bool {
        let Some(tx) = self.handle.reconfigure.lock().take() else { return false };
        self.set_outbound_state(ConnectionState::Configuration);
        tx.send(cookie).is_ok()
    }

    /// Writes `bytes` to the socket unframed, then closes (`sendFlushAndClose` of the legacy ping reply).
    pub fn send_raw(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(Outbound::Raw(bytes));
        let _ = self.tx.send(Outbound::Close);
    }
}

/// Encodes `VarInt id + body`, turning an encoder panic into its message.
pub fn try_encode_payload<P: Packet>(packet: &P) -> Result<Arc<[u8]>, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buf = Vec::with_capacity(64);
        write_varint(&mut buf, P::ID);
        packet.encode(&mut buf);
        Arc::<[u8]>::from(buf)
    }))
    .map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "encoder panicked".to_owned())
    })
}

/// `Util.getMillis()`.
pub fn get_millis() -> i64 {
    crate::server::shared::get_nanos() / 1_000_000
}

/// The disconnect packet of `state` (`VarInt id + body`), if that protocol has one.
fn disconnect_payload(state: ConnectionState, reason: &Component) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    match state {
        ConnectionState::Handshake | ConnectionState::Status => return None,
        ConnectionState::Login => {
            write_varint(&mut buf, login::ClientboundLoginDisconnectPacket::ID);
            login::ClientboundLoginDisconnectPacket { reason: mc_protocol::codec::JsonComponent(reason.clone()) }.encode(&mut buf);
        }
        ConnectionState::Configuration => {
            write_varint(&mut buf, configuration::ClientboundDisconnectPacket::ID);
            configuration::ClientboundDisconnectPacket { reason: reason.clone() }.encode(&mut buf);
        }
        ConnectionState::Play => {
            write_varint(&mut buf, play::ClientboundDisconnectPacket::ID);
            play::ClientboundDisconnectPacket { reason: reason.clone() }.encode(&mut buf);
        }
    }
    Some(buf)
}

/// The writer task: owns the write half, frames/compresses/encrypts queued payloads in batches.
pub(crate) async fn run_writer(mut socket: OwnedWriteHalf, mut rx: UnboundedReceiver<Outbound>, handle: Arc<ConnectionHandle>) {
    let mut encoder = FrameEncoder::new();
    let mut state = ConnectionState::Handshake;
    let mut buf: Vec<u8> = Vec::new();
    // Data was encoded into `buf` but no flush was requested yet; `deadline` = when `Connection.tick` would flush it.
    let mut deadline: Option<Instant> = None;
    let mut closing = false;
    let mut fault: Option<NetError> = None;

    while !closing {
        let msg = match deadline {
            Some(at) => tokio::select! {
                m = rx.recv() => m,
                () = tokio::time::sleep_until(at) => Some(Outbound::Flush),
            },
            None => rx.recv().await,
        };
        let Some(first) = msg else { break };
        let mut flush = false;
        let mut next = Some(first);
        while let Some(m) = next.take().or_else(|| rx.try_recv().ok()) {
            match m {
                Outbound::Payload(p) => {
                    flush = true;
                    if let Err(e) = encoder.encode(&p, &mut buf) {
                        fault = Some(e);
                    }
                }
                Outbound::Write(p) => {
                    if let Err(e) = encoder.encode(&p, &mut buf) {
                        fault = Some(e);
                    }
                    deadline.get_or_insert_with(|| Instant::now() + Duration::from_millis(50));
                }
                Outbound::Flush => flush = true,
                Outbound::SetCompression(t) => encoder.set_compression(t),
                Outbound::EnableEncryption(k) => encoder.enable_encryption(k),
                Outbound::SetState(s) => state = s,
                Outbound::Disconnect(reason) => {
                    if let Some(p) = disconnect_payload(state, &reason) {
                        let _ = encoder.encode(&p, &mut buf);
                    }
                    closing = true;
                }
                Outbound::Close => closing = true,
                Outbound::Raw(bytes) => {
                    buf.extend_from_slice(&bytes);
                    flush = true;
                }
            }
            if closing || fault.is_some() {
                break;
            }
        }
        if let Some(e) = fault.take() {
            // `exceptionCaught` on the encoder: tell the client, then close.
            let reason = generic_reason(internal_exception_text(&e));
            handle.set_disconnection_details(DisconnectionDetails::new(reason.clone()));
            handle.set_read_only();
            if let Some(p) = disconnect_payload(state, &reason) {
                let _ = encoder.encode(&p, &mut buf);
            }
            closing = true;
        }
        if flush || closing {
            deadline = None;
            if (!buf.is_empty() && socket.write_all(&buf).await.is_err()) || socket.flush().await.is_err() {
                break;
            }
            buf.clear();
        }
    }
    let _ = socket.shutdown().await;
    handle.mark_closed();
}

/// Why `FrameReader::next_payload` produced no payload.
#[derive(Debug)]
pub enum ReadError {
    /// The peer closed the stream (`channelInactive`).
    Eof,
    /// `ReadTimeoutHandler`: nothing was read for 30 seconds.
    Timeout,
    /// An I/O error or a framing/decoding error: netty's `exceptionCaught`.
    Net(NetError),
}

/// The read half: socket reads with the 30 s timeout, then `[decrypt] -> split -> [decompress]`.
pub struct FrameReader {
    socket: OwnedReadHalf,
    decoder: FrameDecoder,
    chunk: Box<[u8; 8192]>,
    last_read: Instant,
}

impl FrameReader {
    pub fn new(socket: OwnedReadHalf) -> Self {
        Self { socket, decoder: FrameDecoder::new(), chunk: Box::new([0; 8192]), last_read: Instant::now() }
    }

    /// Inbound half of `Connection.setupCompression`.
    pub fn set_compression(&mut self, threshold: i32, validate: bool) {
        self.decoder.set_compression(threshold, validate);
    }

    /// Inbound half of `Connection.setEncryptionKey`.
    pub fn enable_encryption(&mut self, secret: [u8; 16]) {
        self.decoder.enable_encryption(secret);
    }

    /// Reads the first socket buffer without decoding it, so `LegacyQueryHandler` can look at it. The caller must
    /// either answer it as a legacy ping or hand it back with [`FrameReader::push_first`].
    pub async fn read_first(&mut self) -> Result<Vec<u8>, ReadError> {
        let n = self.read_socket().await?;
        Ok(self.chunk[..n].to_vec())
    }

    /// Feeds the buffer returned by [`FrameReader::read_first`] to the frame decoder.
    pub fn push_first(&mut self, first: &[u8]) {
        self.decoder.push(first);
    }

    async fn read_socket(&mut self) -> Result<usize, ReadError> {
        match tokio::time::timeout_at(self.last_read + READ_TIMEOUT, self.socket.read(&mut self.chunk[..])).await {
            Err(_) => Err(ReadError::Timeout),
            Ok(Err(e)) => Err(ReadError::Net(NetError::Io(e))),
            Ok(Ok(0)) => Err(ReadError::Eof),
            Ok(Ok(n)) => {
                self.last_read = Instant::now();
                Ok(n)
            }
        }
    }

    /// The next payload (`VarInt id + body`). Cancel-safe: partial frames and the read-timeout clock stay in `self`.
    pub async fn next_payload(&mut self) -> Result<Vec<u8>, ReadError> {
        loop {
            if let Some(payload) = self.decoder.next_frame().map_err(ReadError::Net)? {
                return Ok(payload);
            }
            let n = self.read_socket().await?;
            self.decoder.push(&self.chunk[..n]);
        }
    }
}

/// `Connection.exceptionCaught` text for a read/decode failure: `Internal Exception: <cause.toString()>`.
pub fn internal_exception_text(error: &NetError) -> String {
    let cause = match error {
        NetError::Io(e) => format!("java.io.IOException: {}", io_error_text(e)),
        NetError::Eof => "java.io.IOException: end of stream".to_owned(),
        NetError::CorruptedFrame(m) => format!("io.netty.handler.codec.CorruptedFrameException: {m}"),
        NetError::Decoder(m) => format!("io.netty.handler.codec.DecoderException: {m}"),
        NetError::Encoder(m) => format!("io.netty.handler.codec.EncoderException: {m}"),
        NetError::Decode(e) => {
            let m = e.to_string();
            const PREFIX: &str = "Failed to decode packet '";
            if let Some(end) = m.strip_prefix(PREFIX).and_then(|r| r.find('\'')) {
                // DecoderException(message, cause).toString() has the message only.
                format!("io.netty.handler.codec.DecoderException: {}", &m[..PREFIX.len() + end + 1])
            } else if m.starts_with("Packet ") && m.contains("was larger than I expected") {
                format!("io.netty.handler.codec.DecoderException: java.io.IOException: {m}")
            } else {
                format!("io.netty.handler.codec.DecoderException: {m}")
            }
        }
    };
    format!("Internal Exception: {cause}")
}

/// `Component.translatable("disconnect.genericReason", "Internal Exception: " + cause)`.
pub fn generic_reason(text: String) -> Component {
    Component::translatable("disconnect.genericReason", vec![mc_text::TranslatableArg::String(text)])
}

/// The JDK's message for an I/O error (`strerror` without Rust's ` (os error N)` suffix).
fn io_error_text(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(i) => text[..i].to_owned(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, TcpStream};

    /// `ReadTimeoutHandler(30)`: a silent peer is dropped after 30 s, a chatty one (any bytes) restarts the clock.
    #[tokio::test(start_paused = true)]
    async fn read_timeout_is_30_seconds_since_the_last_read() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
        let (server_side, _) = listener.accept().await.unwrap();
        let (read, _write) = server_side.into_split();
        let mut reader = FrameReader::new(read);
        let started = Instant::now();

        // A length prefix without its body at t = 20 s keeps the connection alive until t = 50 s.
        let writer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            client.write_all(&[0x05]).await.unwrap();
            client
        });
        assert!(matches!(reader.next_payload().await, Err(ReadError::Timeout)));
        assert_eq!(started.elapsed(), Duration::from_secs(50));
        let _client = writer.await.unwrap();
    }

    #[test]
    fn java_socket_address_forms() {
        assert_eq!(java_socket_address_string(&"127.0.0.1:25565".parse().unwrap()), "/127.0.0.1:25565");
        assert_eq!(java_socket_address_string(&"[::1]:25565".parse().unwrap()), "/[0:0:0:0:0:0:0:1]:25565");
        assert_eq!(java_socket_address_string(&"[::ffff:1.2.3.4]:1".parse().unwrap()), "/1.2.3.4:1");
        assert_eq!(java_host_address(&"2001:db8::ff00:42:8329".parse().unwrap(), false), "2001:db8:0:0:0:ff00:42:8329");
    }

    #[test]
    fn io_error_text_drops_the_os_suffix() {
        let e = std::io::Error::from_raw_os_error(104);
        assert_eq!(io_error_text(&e), "Connection reset by peer");
    }
}
