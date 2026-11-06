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
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode, header};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

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
    heartbeat: Mutex<Option<JoinHandle<()>>>,
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
        if let Some(h) = self.heartbeat.lock().take() {
            h.abort();
        }
        self.connections.senders.lock().clear();
    }

    /// `scheduleHeartbeat`: (re)schedule `statusHeartbeat` every `period` seconds (`<= 0` cancels it).
    pub fn schedule_heartbeat(&self, service: Arc<dyn NotificationService>, period: i64) -> bool {
        let mut slot = self.heartbeat.lock();
        if let Some(h) = slot.take() {
            h.abort();
        }
        if period > 0 {
            let every = Duration::from_secs(period as u64);
            *slot = Some(self.runtime.spawn(async move {
                let mut ticker =
                    tokio::time::interval_at(tokio::time::Instant::now() + every, every);
                loop {
                    ticker.tick().await;
                    let service = service.clone();
                    let _ = tokio::task::spawn_blocking(move || service.status_heartbeat()).await;
                }
            }));
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

fn header_str<'a>(request: &'a Request, name: header::HeaderName) -> Option<&'a str> {
    request.headers().get(name).and_then(|v| v.to_str().ok())
}

fn unauthorized(reason: &'static str) -> ErrorResponse {
    let mut response = ErrorResponse::new(Some(unauthorized_body(reason)));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CONNECTION, HeaderValue::from_static("close"));
    response
}

async fn serve_connection<S>(
    stream: S,
    peer: SocketAddr,
    auth: Arc<AuthenticationHandler>,
    connections: Arc<Connections>,
    api: Arc<dyn ManagementApi>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let callback = |request: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
        let check = auth.check(
            header_str(request, header::AUTHORIZATION),
            header_str(request, header::SEC_WEBSOCKET_PROTOCOL),
            header_str(request, header::ORIGIN),
        );
        match check {
            SecurityCheckResult::Denied(reason) => {
                log::debug!(
                    "Authentication rejected for connection with ip {}: {reason}",
                    peer.ip()
                );
                Err(unauthorized(reason))
            }
            SecurityCheckResult::Allowed {
                token_in_ws_protocol,
            } => {
                if token_in_ws_protocol {
                    response.headers_mut().insert(
                        header::SEC_WEBSOCKET_PROTOCOL,
                        HeaderValue::from_static(AuthenticationHandler::subprotocol_value()),
                    );
                }
                Ok(response)
            }
        }
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(65536))
        .max_frame_size(Some(65536));
    let Ok(mut ws) =
        tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config)).await
    else {
        return;
    };

    let client = ClientInfo {
        connection_id: CONNECTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst) + 1,
    };
    rpc_log(client, format!("Management connection opened for /{peer}"));
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
    rpc_log(client, format!("Management connection closed for /{peer}"));
    let _ = ws.close(None).await;
}
