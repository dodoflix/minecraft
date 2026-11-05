//! Spawns the real vanilla server (`ref/server.jar`) on a random port in a temp dir and offers raw-connection helpers.
#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use mc_protocol::net::Connection;
use mc_protocol::packet::{packet_name, ConnectionState, PacketFlow};
use mc_protocol::packets::handshake::*;
use mc_protocol::PROTOCOL_VERSION;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::timeout;

pub struct VanillaServer {
    child: Child,
    pub port: u16,
    _dir: PathBuf,
    stdin: tokio::process::ChildStdin,
}

impl Drop for VanillaServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = std::fs::remove_dir_all(&self._dir);
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

pub async fn start_vanilla(online_mode: bool) -> Option<VanillaServer> {
    let jar = repo_root().join("ref/server.jar");
    if !jar.exists() || std::process::Command::new("java").arg("-version").output().is_err() {
        eprintln!("skipping: java or ref/server.jar not available");
        return None;
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("mc-protocol-smoke-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("eula.txt"), "eula=true\n").unwrap();
    std::fs::write(
        dir.join("server.properties"),
        format!(
            "online-mode={online_mode}\nserver-port={port}\nserver-ip=127.0.0.1\ngenerate-structures=false\nwhite-list=false\nenforce-whitelist=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=2\nsimulation-distance=2\n\
             enable-rcon=false\nenable-query=false\nrate-limit=0\n"
        ),
    )
    .unwrap();
    // Run through a differently named link so other tooling that `pkill -f server.jar` leaves us alone.
    let link = dir.join("smoke-vanilla.jar");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&jar, &link).unwrap();
    let mut child = Command::new("java")
        .args(["-Xmx1G", "-jar"])
        .arg(&link)
        .arg("nogui")
        .current_dir(&dir)
        .stdin(Stdio::piped()) // EOF on stdin makes the vanilla console stop the server
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn java");
    let stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let ready = timeout(Duration::from_secs(180), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line.contains("Done (") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(ready, "vanilla server did not finish starting");
    // keep draining so the child never blocks on a full pipe
    tokio::spawn(async move { while let Ok(Some(l)) = lines.next_line().await { let _ = l; } });
    // The listener accepts at "Done", but the server thread is still finishing startup work and drops early connections.
    tokio::time::sleep(Duration::from_secs(3)).await;
    Some(VanillaServer { child, port, _dir: dir, stdin })
}

pub async fn connect(port: u16) -> Connection<TcpStream> {
    Connection::new(TcpStream::connect(("127.0.0.1", port)).await.unwrap())
}

macro_rules! send {
    ($conn:expr, $packet:expr) => {{
        let mut buf = Vec::new();
        $packet.encode_payload(&mut buf).unwrap();
        $conn.write_payload(&buf).await.unwrap();
        $conn.flush().await.unwrap();
    }};
}
pub(crate) use send;

pub async fn handshake(conn: &mut Connection<TcpStream>, port: u16, intention: ClientIntent) {
    send!(
        conn,
        ServerboundHandshakePacket::ClientIntention(ClientIntentionPacket {
            protocol_version: PROTOCOL_VERSION,
            host_name: "localhost".into(),
            port,
            intention,
        })
    );
}

pub async fn read(conn: &mut Connection<TcpStream>) -> Vec<u8> {
    timeout(Duration::from_secs(30), conn.read_payload()).await.expect("read timeout").expect("read")
}

pub fn log_id(state: ConnectionState, payload: &[u8]) {
    let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
    println!("<- {} clientbound id={id} {}", state.id(), packet_name(state, PacketFlow::Clientbound, id).unwrap_or("?"));
}


impl VanillaServer {
    /// Runs a console command (as the server console: no permissions needed).
    pub async fn command(&mut self, cmd: &str) {
        self.stdin.write_all(format!("{cmd}\n").as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
    }
}
