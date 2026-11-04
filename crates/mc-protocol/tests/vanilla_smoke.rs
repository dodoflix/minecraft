//! Real-network smoke test against the vanilla server (`ref/server.jar`): status ping, offline login
//! through configuration, and an online-mode encryption handshake. Skipped when java or the jar is missing.
//! Run with `-- --nocapture` to see the packet-id log.
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use mc_protocol::auth::{digest_server_id, encrypt_with_public_key, generate_secret};
use mc_protocol::net::Connection;
use mc_protocol::packet::{packet_name, ConnectionState, PacketFlow};
use mc_protocol::packets::{configuration::*, handshake::*, login::*, status::*};
use mc_protocol::PROTOCOL_VERSION;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::timeout;
use uuid::Uuid;

struct VanillaServer {
    child: Child,
    port: u16,
    _dir: PathBuf,
    _stdin: tokio::process::ChildStdin,
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

async fn start_vanilla(online_mode: bool) -> Option<VanillaServer> {
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
    Some(VanillaServer { child, port, _dir: dir, _stdin: stdin })
}

async fn connect(port: u16) -> Connection<TcpStream> {
    Connection::new(TcpStream::connect(("127.0.0.1", port)).await.unwrap())
}

macro_rules! send {
    ($conn:expr, $enum:ident :: $variant:ident($packet:expr)) => {{
        let mut buf = Vec::new();
        $enum::$variant($packet).encode_payload(&mut buf);
        $conn.write_payload(&buf).await.unwrap();
        $conn.flush().await.unwrap();
    }};
}

async fn handshake(conn: &mut Connection<TcpStream>, port: u16, intention: ClientIntent) {
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

async fn read(conn: &mut Connection<TcpStream>) -> Vec<u8> {
    timeout(Duration::from_secs(30), conn.read_payload()).await.expect("read timeout").expect("read")
}

fn log_id(state: ConnectionState, payload: &[u8]) {
    let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
    println!("<- {} clientbound id={id} {}", state.id(), packet_name(state, PacketFlow::Clientbound, id).unwrap_or("?"));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_and_offline_login_to_configuration() {
    let Some(server) = start_vanilla(false).await else { return };
    let port = server.port;

    // ---- status
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Status).await;
    send!(c, ServerboundStatusPacket::StatusRequest(ServerboundStatusRequestPacket));
    let payload = read(&mut c).await;
    log_id(ConnectionState::Status, &payload);
    let ClientboundStatusPacket::StatusResponse(resp) = ClientboundStatusPacket::decode_payload(&payload).unwrap() else {
        panic!("expected status response")
    };
    println!("status: {}", resp.status.to_json());
    assert_eq!(resp.status.version.as_ref().unwrap().protocol, PROTOCOL_VERSION);
    assert_eq!(resp.status.players.as_ref().unwrap().online, 0);
    send!(c, ServerboundStatusPacket::PingRequest(ServerboundPingRequestPacket { time: 0x1234_5678_9abc }));
    let payload = read(&mut c).await;
    log_id(ConnectionState::Status, &payload);
    assert_eq!(
        ClientboundStatusPacket::decode_payload(&payload).unwrap(),
        ClientboundStatusPacket::PongResponse(ClientboundPongResponsePacket { time: 0x1234_5678_9abc })
    );

    // ---- login (offline) -> configuration
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Login).await;
    let name = "SmokeBot";
    let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
    send!(c, ServerboundLoginPacket::Hello(ServerboundHelloPacket { name: name.into(), profile_id: uuid }));
    let profile = loop {
        let payload = read(&mut c).await;
        log_id(ConnectionState::Login, &payload);
        match ClientboundLoginPacket::decode_payload(&payload).unwrap() {
            ClientboundLoginPacket::LoginCompression(p) => {
                println!("compression threshold {}", p.compression_threshold);
                c.set_compression(p.compression_threshold, false);
            }
            ClientboundLoginPacket::LoginFinished(p) => break p.game_profile,
            other => panic!("unexpected login packet {other:?}"),
        }
    };
    assert_eq!(profile.name, name);
    send!(c, ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket));

    send!(
        c,
        ServerboundConfigurationPacket::ClientInformation(ServerboundClientInformationPacket { information: Default::default() })
    );
    let mut seen = Vec::new();
    loop {
        let payload = read(&mut c).await;
        log_id(ConnectionState::Configuration, &payload);
        let packet = ClientboundConfigurationPacket::decode_payload(&payload).unwrap();
        seen.push(packet.name());
        match packet {
            ClientboundConfigurationPacket::SelectKnownPacks(_) => {
                send!(c, ServerboundConfigurationPacket::SelectKnownPacks(ServerboundSelectKnownPacks { known_packs: vec![] }))
            }
            ClientboundConfigurationPacket::KeepAlive(p) => {
                send!(c, ServerboundConfigurationPacket::KeepAlive(ServerboundKeepAlivePacket { id: p.id }))
            }
            ClientboundConfigurationPacket::Ping(p) => {
                send!(c, ServerboundConfigurationPacket::Pong(ServerboundPongPacket { id: p.id }))
            }
            ClientboundConfigurationPacket::FinishConfiguration(_) => {
                send!(c, ServerboundConfigurationPacket::FinishConfiguration(ServerboundFinishConfigurationPacket));
                break;
            }
            _ => {}
        }
    }
    for expected in ["minecraft:custom_payload", "minecraft:registry_data", "minecraft:update_tags", "minecraft:select_known_packs"] {
        assert!(seen.contains(&expected), "never received {expected}; got {seen:?}");
    }
    // The next packet is the first play packet (`login`, id 50).
    let payload = read(&mut c).await;
    log_id(ConnectionState::Play, &payload);
    assert_eq!(mc_protocol::codec::read_varint(&mut &payload[..]).unwrap(), 50);
}

#[tokio::test(flavor = "multi_thread")]
async fn online_mode_encryption_handshake() {
    let Some(server) = start_vanilla(true).await else { return };
    let port = server.port;
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Login).await;
    send!(c, ServerboundLoginPacket::Hello(ServerboundHelloPacket { name: "SmokeBot".into(), profile_id: Uuid::nil() }));
    let payload = read(&mut c).await;
    log_id(ConnectionState::Login, &payload);
    let ClientboundLoginPacket::Hello(hello) = ClientboundLoginPacket::decode_payload(&payload).unwrap() else { panic!("expected hello") };
    assert!(hello.should_authenticate);
    let secret = generate_secret();
    println!("server hash would be {}", digest_server_id(&hello.server_id, &secret, &hello.public_key));
    let key = ServerboundKeyPacket {
        keybytes: encrypt_with_public_key(&hello.public_key, &secret).unwrap(),
        encrypted_challenge: encrypt_with_public_key(&hello.public_key, &hello.challenge).unwrap(),
    };
    send!(c, ServerboundLoginPacket::Key(key));
    c.enable_encryption(secret);
    // The server accepted our key (challenge + secret) iff it can talk to us over the encrypted
    // stream: it then fails the session-server check (no real Mojang account) with a login disconnect,
    // or, if the session server is unreachable, with an "authservers down" disconnect.
    let payload = read(&mut c).await;
    log_id(ConnectionState::Login, &payload);
    match ClientboundLoginPacket::decode_payload(&payload).unwrap() {
        ClientboundLoginPacket::LoginDisconnect(d) => println!("disconnect: {}", d.reason.0.get_string()),
        ClientboundLoginPacket::LoginCompression(_) => println!("(server proceeded without authentication failure)"),
        other => panic!("unexpected {other:?}"),
    }
}
