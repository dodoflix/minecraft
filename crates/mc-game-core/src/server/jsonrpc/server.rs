//! `ManagementServer` + `Connection`: the websocket listener (optional TLS), authentication on the HTTP upgrade and
//! the per-connection message loop.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{HeaderValue, header};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use super::api::{ClientInfo, ManagementApi};
use super::methods::{Dispatcher, Outbound, rpc_log};
use super::security::{AuthenticationHandler, SecurityCheckResult, unauthorized_body};
use crate::server::notifications::NotificationService;

static CONNECTION_ID_COUNTER: AtomicI32 = AtomicI32::new(0);

/// The open connections (`ManagementServer.connections`): each is the sending half of its outbound queue.
#[derive(Default)]
pub struct Connections {
    senders: Mutex<HashMap<i32, UnboundedSender<String>>>,
}

impl Connections {
    pub fn new() -> Arc<Connections> {
        Arc::default()
    }

    /// Send one text frame to every open connection (`forEachConnection(c -> c.sendNotification(..))`).
    pub fn broadcast(&self, text: &str) {
        for sender in self.senders.lock().values() {
            let _ = sender.send(text.to_owned());
        }
    }

    pub fn len(&self) -> usize {
        self.senders.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn add(&self, id: i32, sender: UnboundedSender<String>) {
        self.senders.lock().insert(id, sender);
    }

    fn remove(&self, id: i32) {
        self.senders.lock().remove(&id);
    }
}

pub struct ManagementServer {
    host: String,
    port: i32,
    auth: Arc<AuthenticationHandler>,
    runtime: tokio::runtime::Runtime,
    connections: Arc<Connections>,
    listener_task: Option<JoinHandle<()>>,
    local_port: Option<u16>,
    /// `ManagementServer.heartbeat`: the last scheduled task and whether it was canceled (a canceled task stays
    /// in the field).
    heartbeat: Mutex<Option<(JoinHandle<()>, bool)>>,
}

impl ManagementServer {
    pub fn new(host: &str, port: i32, auth: AuthenticationHandler) -> io::Result<ManagementServer> {
        static THREAD_ID: AtomicUsize = AtomicUsize::new(0);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name_fn(|| {
                format!(
                    "Management server IO #{}",
                    THREAD_ID.fetch_add(1, Ordering::SeqCst)
                )
            })
            .build()?;
        Ok(ManagementServer {
            host: host.to_owned(),
            port,
            auth: Arc::new(auth),
            runtime,
            connections: Connections::new(),
            listener_task: None,
            local_port: None,
            heartbeat: Mutex::new(None),
        })
    }

    pub fn connections(&self) -> Arc<Connections> {
        self.connections.clone()
    }

    /// `getPort`: the bound port once started, else the configured one.
    pub fn get_port(&self) -> i32 {
        self.local_port.map_or(self.port, i32::from)
    }

    /// `start(api, sslContext)`: bind and serve. Logs `Json-RPC Management connection listening on host:port`.
    pub fn start(
        &mut self,
        api: Arc<dyn ManagementApi>,
        tls: Option<TlsAcceptor>,
    ) -> io::Result<()> {
        let addr: SocketAddr = (self.host.as_str(), self.port as u16)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "management server host does not resolve",
                )
            })?;
        let listener = self.runtime.block_on(TcpListener::bind(addr))?;
        self.local_port = Some(listener.local_addr()?.port());
        let (auth, connections) = (self.auth.clone(), self.connections.clone());
        self.listener_task =
            Some(
                self.runtime
                    .spawn(accept_loop(listener, auth, connections, api, tls)),
            );
        log::info!(
            "Json-RPC Management connection listening on {}:{}",
            self.host,
            self.get_port()
        );
        Ok(())
    }

    /// `stop`: close the listener and every connection.
    pub fn stop(&mut self) {
        if let Some(task) = self.listener_task.take() {
            task.abort();
        }
        if let Some((h, _)) = self.heartbeat.lock().take() {
            h.abort();
        }
        self.connections.senders.lock().clear();
    }

    /// `scheduleHeartbeat`: (re)schedule `statusHeartbeat` every `period` seconds (`<= 0` only cancels). Like vanilla's,
    /// a task that was canceled before cannot be canceled again, which refuses every later change.
    pub fn schedule_heartbeat(&self, service: Arc<dyn NotificationService>, period: i64) -> bool {
        let mut slot = self.heartbeat.lock();
        if let Some((task, canceled)) = slot.as_mut() {
            if *canceled {
                log::warn!(
                    "The existing heartbeat was not canceled and the new heartbeat of {period} seconds has not been applied."
                );
                return false;
            }
            task.abort();
            *canceled = true;
        }
        if period > 0 {
            let every = Duration::from_secs(period as u64);
            *slot = Some((
                self.runtime.spawn(async move {
                    let mut ticker =
                        tokio::time::interval_at(tokio::time::Instant::now() + every, every);
                    loop {
                        ticker.tick().await;
                        let service = service.clone();
                        let _ =
                            tokio::task::spawn_blocking(move || service.status_heartbeat()).await;
                    }
                }),
                false,
            ));
        }
        true
    }
}

async fn accept_loop(
    listener: TcpListener,
    auth: Arc<AuthenticationHandler>,
    connections: Arc<Connections>,
    api: Arc<dyn ManagementApi>,
    tls: Option<TlsAcceptor>,
) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            continue;
        };
        let _ = stream.set_nodelay(true);
        let (auth, connections, api, tls) =
            (auth.clone(), connections.clone(), api.clone(), tls.clone());
        tokio::spawn(async move {
            match tls {
                Some(acceptor) => match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        serve_connection(tls_stream, peer, auth, connections, api).await
                    }
                    Err(e) => log::debug!("TLS handshake with {peer} failed: {e}"),
                },
                None => serve_connection(stream, peer, auth, connections, api).await,
            }
        });
    }
}

/// The head of an HTTP request as netty's `HttpServerCodec` hands it to the pipeline.
struct RequestHead {
    method: String,
    uri: String,
    /// Lower-case names, the first value of each.
    headers: HashMap<String, String>,
}

impl RequestHead {
    fn parse(head: &[u8]) -> Option<RequestHead> {
        let text = String::from_utf8_lossy(head);
        let mut lines = text.split("\r\n");
        let mut request_line = lines.next()?.split(' ');
        let (method, uri) = (request_line.next()?, request_line.next()?);
        let mut headers = HashMap::new();
        for line in lines.take_while(|l| !l.is_empty()) {
            if let Some((name, value)) = line.split_once(':') {
                headers
                    .entry(name.trim().to_ascii_lowercase())
                    .or_insert_with(|| value.trim().to_owned());
            }
        }
        Some(RequestHead {
            method: method.to_owned(),
            uri: uri.to_owned(),
            headers,
        })
    }

    fn header(&self, name: header::HeaderName) -> Option<&str> {
        self.headers.get(name.as_str()).map(String::as_str)
    }

    /// `HttpHeaders.containsValue(name, value, true)`: a comma separated token, case-insensitive.
    fn has_token(&self, name: header::HeaderName, token: &str) -> bool {
        self.header(name)
            .is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
    }
}

/// Reads up to the end of the request head (`\r\n\r\n`); `None` when the peer closes first or sends more than the
/// aggregator's 65536 bytes.
async fn read_head<S: AsyncRead + Unpin>(stream: &mut S) -> Option<Vec<u8>> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream.read(&mut chunk).await.ok().filter(|n| *n > 0)?;
        let from = head.len().saturating_sub(3);
        head.extend_from_slice(&chunk[..n]);
        if head[from..].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if head.len() > 65536 {
            return None;
        }
    }
    Some(head)
}

/// A stream that first yields bytes already read from the inner stream.
struct Replay<S> {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: S,
}

impl<S: AsyncRead + Unpin> AsyncRead for Replay<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let this = &mut *self;
        let rest = &this.prefix.get_ref()[this.prefix.position() as usize..];
        if !rest.is_empty() {
            let n = rest.len().min(buf.remaining());
            buf.put_slice(&rest[..n]);
            this.prefix.set_position(this.prefix.position() + n as u64);
            return std::task::Poll::Ready(Ok(()));
        }
        std::pin::Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Replay<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// What the pipeline in front of the websocket codec does with a request: `AuthenticationHandler` first, then the
/// handshake handler for `/` (other paths pass through unanswered). `Ok(true)` when the websocket upgrade may go on
/// (the sub-protocol answer is wanted); `Err` is the raw response, or nothing to write.
fn screen_request(
    auth: &AuthenticationHandler,
    head: &RequestHead,
    peer: SocketAddr,
) -> Result<bool, Option<String>> {
    let token_in_ws_protocol = match auth.check(
        head.header(header::AUTHORIZATION),
        head.header(header::SEC_WEBSOCKET_PROTOCOL),
        head.header(header::ORIGIN),
    ) {
        SecurityCheckResult::Denied(reason) => {
            log::debug!(
                "Authentication rejected for connection with ip {}: {reason}",
                peer.ip()
            );
            let body = unauthorized_body(reason);
            return Err(Some(format!(
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )));
        }
        SecurityCheckResult::Allowed {
            token_in_ws_protocol,
        } => token_in_ws_protocol,
    };
    if head.uri != "/" {
        return Err(None);
    }
    let refusal = if head.method != "GET" {
        Some(format!(
            "Invalid WebSocket handshake method: {}",
            head.method
        ))
    } else if !head.has_token(header::CONNECTION, "upgrade")
        || !head.has_token(header::UPGRADE, "websocket")
    {
        Some("not a WebSocket handshake request: missing upgrade".to_owned())
    } else {
        None
    };
    match refusal {
        Some(message) => Err(Some(format!("HTTP/1.1 400 Bad Request\r\n\r\n{message}"))),
        None => Ok(token_in_ws_protocol),
    }
}

async fn serve_connection<S>(
    mut stream: S,
    peer: SocketAddr,
    auth: Arc<AuthenticationHandler>,
    connections: Arc<Connections>,
    api: Arc<dyn ManagementApi>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // the `Connection` handler exists for every channel: its id is taken and its opening and closing are logged even
    // when authentication refuses the request
    let client = ClientInfo {
        connection_id: CONNECTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst) + 1,
    };
    rpc_log(client, format!("Management connection opened for /{peer}"));
    serve_requests(&mut stream, peer, &auth, &connections, api, client).await;
    rpc_log(client, format!("Management connection closed for /{peer}"));
}

async fn serve_requests<S>(
    stream: &mut S,
    peer: SocketAddr,
    auth: &AuthenticationHandler,
    connections: &Connections,
    api: Arc<dyn ManagementApi>,
    client: ClientInfo,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let Some(head) = read_head(stream).await else {
        return;
    };
    let Some(request) = RequestHead::parse(&head) else {
        return;
    };
    let token_in_ws_protocol = match screen_request(auth, &request, peer) {
        Ok(token) => token,
        Err(response) => {
            if let Some(response) = response {
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            } else {
                // the request is passed down the pipeline and nothing answers it: the channel stays open
                let mut sink = [0u8; 512];
                while stream.read(&mut sink).await.is_ok_and(|n| n > 0) {}
            }
            return;
        }
    };
    #[expect(
        clippy::result_large_err,
        reason = "the tungstenite handshake `Callback` signature fixes `ErrorResponse` as the error type"
    )]
    let callback = |_: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
        if token_in_ws_protocol {
            response.headers_mut().insert(
                header::SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_static(AuthenticationHandler::subprotocol_value()),
            );
        }
        Ok(response)
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(65536))
        .max_frame_size(Some(65536));
    let replay = Replay {
        prefix: std::io::Cursor::new(head),
        inner: stream,
    };
    let Ok(mut ws) =
        tokio_tungstenite::accept_hdr_async_with_config(replay, callback, Some(config)).await
    else {
        return;
    };
    let (tx, mut rx) = unbounded_channel::<String>();
    connections.add(client.connection_id, tx);
    let dispatcher = Arc::new(Dispatcher::new(api));
    loop {
        tokio::select! {
            outbound = rx.recv() => {
                let Some(text) = outbound else { break };
                if ws.send(Message::text(text)).await.is_err() {
                    break;
                }
            }
            incoming = ws.next() => {
                let Some(Ok(message)) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        let dispatcher = dispatcher.clone();
                        let result = tokio::task::spawn_blocking(move || dispatcher.handle_text(text.as_str(), client)).await;
                        match result {
                            Ok(Outbound::Reply(reply)) => {
                                if ws.send(Message::text(reply)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(Outbound::None) => {}
                            Ok(Outbound::Close) | Err(_) => break,
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }
    connections.remove(client.connection_id);
    // netty closes a channel with the status `NORMAL_CLOSURE` (1000, "Bye")
    let _ = ws
        .close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "Bye".into(),
        }))
        .await;
}
