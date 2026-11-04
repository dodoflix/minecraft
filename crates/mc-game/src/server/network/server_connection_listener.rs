//! `ServerConnectionListener`: the tokio runtime (threads named like netty's `Netty Epoll Server IO #n`), the TCP
//! listener on `server-ip`:`server-port`, and the list of live connections.

use super::connection_task::{ListenerShared, run_connection};
use super::hooks::NetworkHooks;
use crate::server::MinecraftServer;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub struct ServerConnectionListener {
    hooks: Arc<dyn NetworkHooks>,
    listener: Arc<ListenerShared>,
    runtime: Option<Runtime>,
    /// `ServerConnectionListener.channels` (TCP listeners only).
    channels: Vec<(SocketAddr, JoinHandle<()>)>,
    /// `running`.
    running: bool,
    play_ticker: Option<PlayConnectionTicker>,
}

/// `Connection.tick()` of a connection in the play protocol (`ServerGamePacketListenerImpl.tick` + disconnect
/// handling), implemented by the play listener: called once per server tick for every play connection, in connection
/// order, with the player's UUID; a connection is dropped from the list one tick after it closed.
pub type PlayConnectionTicker = fn(&mut MinecraftServer, Uuid);

impl ServerConnectionListener {
    pub fn new(hooks: Arc<dyn NetworkHooks>) -> io::Result<Self> {
        let counter = AtomicUsize::new(0);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(std::thread::available_parallelism().map_or(2, |n| n.get().max(2)))
            .thread_name_fn(move || format!("Netty Epoll Server IO #{}", counter.fetch_add(1, Ordering::Relaxed) + 1))
            .enable_all()
            .build()?;
        Ok(Self { hooks, listener: ListenerShared::new(), runtime: Some(runtime), channels: Vec::new(), running: true, play_ticker: None })
    }

    /// `startTcpServerListener`: binds `address:port` (all interfaces when `None`). Returns the bound address.
    pub fn start_tcp_server_listener(&mut self, address: Option<IpAddr>, port: u16) -> io::Result<SocketAddr> {
        let runtime = self.runtime.as_ref().ok_or_else(|| io::Error::other("connection listener stopped"))?;
        let socket = {
            let _guard = runtime.enter();
            match address {
                Some(ip) => bind(SocketAddr::new(ip, port))?,
                // `new InetSocketAddress(port)`: the wildcard address, dual-stack where the OS has IPv6.
                None => bind(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port))
                    .or_else(|_| bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)))?,
            }
        };
        let local = socket.local_addr()?;
        let hooks = self.hooks.clone();
        let listener = self.listener.clone();
        let next_id = Arc::new(AtomicU64::new(1));
        let task = runtime.spawn(async move {
            loop {
                match socket.accept().await {
                    Ok((stream, _)) => {
                        let id = next_id.fetch_add(1, Ordering::Relaxed);
                        tokio::spawn(run_connection(stream, id, hooks.clone(), listener.clone()));
                    }
                    // EMFILE and friends: back off like netty's accept backoff instead of spinning.
                    Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                }
            }
        });
        self.channels.push((local, task));
        Ok(local)
    }

    /// `stopTcpServerListener`: closes the TCP listeners (connections stay).
    pub fn stop_tcp_server_listener(&mut self) {
        for (_, task) in self.channels.drain(..) {
            task.abort();
        }
    }

    /// `stop()`: stops accepting and tears the network runtime down.
    pub fn stop(&mut self) {
        self.running = false;
        self.stop_tcp_server_listener();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Installs the play listener's per-tick hook (set once when the server is built).
    pub fn set_play_connection_ticker(&mut self, ticker: PlayConnectionTicker) {
        self.play_ticker = Some(ticker);
    }

    /// `tick()`: ticks the play connections, forgets closed connections.
    pub fn tick(&mut self, server: &mut MinecraftServer) {
        let play = self.listener.play_snapshot();
        if let Some(ticker) = self.play_ticker {
            for (id, _) in &play {
                ticker(server, *id);
            }
        }
        self.listener.drop_closed_play_connections(&play);
        self.listener.prune();
    }

    /// `getSessionId`.
    pub fn get_session_id(&self) -> Uuid {
        self.listener.session_id()
    }

    /// Number of connections that are still open (all protocol states).
    pub fn connection_count(&self) -> usize {
        self.listener.connection_count()
    }
}

fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
    let std_listener = std::net::TcpListener::bind(addr)?;
    std_listener.set_nonblocking(true)?;
    TcpListener::from_std(std_listener)
}
