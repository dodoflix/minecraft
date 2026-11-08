//! `QueryThreadGs4`: the GameSpy-4 UDP query listener (`enable-query`, `query.port`).

use std::collections::HashMap;
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rand::RngExt;

use super::GenericThread;
use super::pkt_utils::{
    MAX_PACKET_SIZE, NetworkDataOutputStream, int_from_network_byte_array, to_hex_string,
};

const GAME_TYPE: &str = "SMP";
const GAME_ID: &str = "MINECRAFT";
const CHALLENGE_CHECK_INTERVAL: Duration = Duration::from_millis(30_000);
const RESPONSE_CACHE_TIME: Duration = Duration::from_millis(5000);

/// The `ServerInterface` accessors the query listener reads. The constant ones are read once at creation (like
/// vanilla's constructor), the others per request.
pub trait QueryServerInfo: Send + Sync {
    /// `getServerPort`.
    fn server_port(&self) -> i32;
    /// `getServerName` (the motd).
    fn server_name(&self) -> String;
    /// `getMaxPlayers`.
    fn max_players(&self) -> i32;
    /// `getLevelIdName`.
    fn level_id_name(&self) -> String;
    /// `getPlayerCount`.
    fn player_count(&self) -> i32;
    /// `getPlayerNames`.
    fn player_names(&self) -> Vec<String>;
    /// `getServerVersion`.
    fn server_version(&self) -> String;
    /// `getPluginNames` (vanilla: empty).
    fn plugin_names(&self) -> String {
        String::new()
    }
}

struct RequestChallenge {
    time: Instant,
    challenge: i32,
    ident_bytes: [u8; 4],
    challenge_bytes: Vec<u8>,
}

impl RequestChallenge {
    fn new(request: &[u8]) -> RequestChallenge {
        let ident_bytes = [request[3], request[4], request[5], request[6]];
        let ident = String::from_utf8_lossy(&ident_bytes).into_owned();
        let challenge = rand::rng().random_range(0..16_777_216);
        RequestChallenge {
            time: Instant::now(),
            challenge,
            ident_bytes,
            challenge_bytes: format!("\t{ident}{challenge}\u{0}").into_bytes(),
        }
    }
}

struct Responder {
    info: Arc<dyn QueryServerInfo>,
    server_port: i32,
    max_players: i32,
    server_name: String,
    world_name: String,
    host_ip: String,
    valid_challenges: HashMap<std::net::SocketAddr, RequestChallenge>,
    rules_response: NetworkDataOutputStream,
    last_rules_response: Option<Instant>,
    last_challenge_check: Instant,
}

impl Responder {
    fn ident_bytes(&self, src: &std::net::SocketAddr) -> [u8; 4] {
        self.valid_challenges
            .get(src)
            .map_or([0; 4], |c| c.ident_bytes)
    }

    fn valid_challenge(&self, src: &std::net::SocketAddr, data: &[u8]) -> bool {
        self.valid_challenges
            .get(src)
            .is_some_and(|c| c.challenge == int_from_network_byte_array(data, 7, data.len()))
    }

    fn build_rule_response(&mut self, src: &std::net::SocketAddr) -> Vec<u8> {
        let now = Instant::now();
        if self
            .last_rules_response
            .is_some_and(|t| now < t + RESPONSE_CACHE_TIME)
        {
            let mut data = self.rules_response.to_byte_array();
            data[1..5].copy_from_slice(&self.ident_bytes(src));
            return data;
        }
        self.last_rules_response = Some(now);
        let ident = self.ident_bytes(src);
        let r = &mut self.rules_response;
        r.reset();
        r.write(0);
        r.write_bytes(&ident);
        r.write_string("splitnum");
        r.write(128);
        r.write(0);
        r.write_string("hostname");
        r.write_string(&self.server_name);
        r.write_string("gametype");
        r.write_string(GAME_TYPE);
        r.write_string("game_id");
        r.write_string(GAME_ID);
        r.write_string("version");
        r.write_string(&self.info.server_version());
        r.write_string("plugins");
        r.write_string(&self.info.plugin_names());
        r.write_string("map");
        r.write_string(&self.world_name);
        r.write_string("numplayers");
        r.write_string(&self.info.player_count().to_string());
        r.write_string("maxplayers");
        r.write_string(&self.max_players.to_string());
        r.write_string("hostport");
        r.write_string(&self.server_port.to_string());
        r.write_string("hostip");
        r.write_string(&self.host_ip);
        r.write(0);
        r.write(1);
        r.write_string("player_");
        r.write(0);
        for player in self.info.player_names() {
            r.write_string(&player);
        }
        r.write(0);
        r.to_byte_array()
    }

    /// `processPacket`: the datagram to send back, if any.
    fn process_packet(&mut self, data: &[u8], src: std::net::SocketAddr) -> Option<Vec<u8>> {
        let len = data.len();
        log::debug!("Packet len {len} [{src}]");
        if len >= 3 && data[0] == 0xFE && data[1] == 0xFD {
            log::debug!("Packet '{}' [{src}]", to_hex_string(data[2]));
            match data[2] {
                0 => {
                    if !self.valid_challenge(&src, data) {
                        log::debug!("Invalid challenge [{src}]");
                        return None;
                    }
                    if len == 15 {
                        log::debug!("Rules [{src}]");
                        Some(self.build_rule_response(&src))
                    } else {
                        let mut dos = NetworkDataOutputStream::new(MAX_PACKET_SIZE);
                        dos.write(0);
                        dos.write_bytes(&self.ident_bytes(&src));
                        dos.write_string(&self.server_name);
                        dos.write_string(GAME_TYPE);
                        dos.write_string(&self.world_name);
                        dos.write_string(&self.info.player_count().to_string());
                        dos.write_string(&self.max_players.to_string());
                        dos.write_short(self.server_port as i16);
                        dos.write_string(&self.host_ip);
                        log::debug!("Status [{src}]");
                        Some(dos.to_byte_array())
                    }
                }
                9 => {
                    let challenge = RequestChallenge::new(data);
                    let reply = challenge.challenge_bytes.clone();
                    self.valid_challenges.insert(src, challenge);
                    log::debug!("Challenge [{src}]");
                    Some(reply)
                }
                _ => None,
            }
        } else {
            log::debug!("Invalid packet [{src}]");
            None
        }
    }

    /// `pruneChallenges`: every 30 s drop the challenges created before now (i.e. all of them, like vanilla).
    fn prune_challenges(&mut self) {
        let now = Instant::now();
        if now >= self.last_challenge_check + CHALLENGE_CHECK_INTERVAL {
            self.last_challenge_check = now;
            self.valid_challenges.retain(|_, c| !(c.time < now));
        }
    }
}

/// `InetAddress.getLocalHost().getHostAddress()` (first address the host name resolves to).
fn local_host_address() -> Option<String> {
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())?;
    (name.trim(), 0)
        .to_socket_addrs()
        .ok()?
        .next()
        .map(|a| a.ip().to_string())
}

/// `QueryThreadGs4`.
pub struct QueryThreadGs4 {
    thread: GenericThread,
}

impl QueryThreadGs4 {
    /// `QueryThreadGs4.create`: `None` (after logging) for an invalid port or when the socket cannot be bound.
    pub fn create(
        info: Arc<dyn QueryServerInfo>,
        server_ip: &str,
        query_port: i32,
    ) -> Option<QueryThreadGs4> {
        if !(0 < query_port && query_port <= 65535) {
            log::warn!(
                "Invalid query port {query_port} found in server.properties (queries disabled)"
            );
            return None;
        }
        let mut server_ip = server_ip.to_owned();
        let mut host_ip = "0.0.0.0".to_owned();
        if !server_ip.is_empty() && host_ip != server_ip {
            host_ip = server_ip.clone();
        } else {
            server_ip = "0.0.0.0".to_owned();
            match local_host_address() {
                Some(ip) => host_ip = ip,
                None => log::warn!(
                    "Unable to determine local host IP, please set server-ip in server.properties"
                ),
            }
        }
        let socket = match (server_ip.as_str(), query_port as u16)
            .to_socket_addrs()
            .and_then(|mut a| a.next().ok_or_else(|| std::io::Error::other("no address")))
            .and_then(UdpSocket::bind)
            .and_then(|s| {
                s.set_read_timeout(Some(Duration::from_millis(500)))
                    .map(|()| s)
            }) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Unable to initialise query system on {server_ip}:{query_port}\n{e}");
                return None;
            }
        };
        let mut responder = Responder {
            server_port: info.server_port(),
            server_name: info.server_name(),
            max_players: info.max_players(),
            world_name: info.level_id_name(),
            info,
            host_ip,
            valid_challenges: HashMap::new(),
            rules_response: NetworkDataOutputStream::new(MAX_PACKET_SIZE),
            last_rules_response: None,
            last_challenge_check: Instant::now(),
        };
        let mut thread = GenericThread::new("Query Listener");
        let started = thread.start(move |running| {
            log::info!("Query running on {server_ip}:{query_port}");
            run(&socket, &running, &mut responder);
        });
        started.then_some(QueryThreadGs4 { thread })
    }

    pub fn is_running(&self) -> bool {
        self.thread.is_running()
    }

    pub fn stop(&mut self) {
        self.thread.stop();
    }
}

fn run(socket: &UdpSocket, running: &AtomicBool, responder: &mut Responder) {
    let mut buffer = [0u8; MAX_PACKET_SIZE];
    while running.load(Ordering::SeqCst) {
        match socket.recv_from(&mut buffer) {
            Ok((len, src)) => {
                responder.prune_challenges();
                if let Some(reply) = responder.process_packet(&buffer[..len], src) {
                    let _ = socket.send_to(&reply, src);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                responder.prune_challenges()
            }
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
            Err(e) => {
                if running.load(Ordering::SeqCst) {
                    log::warn!("Unexpected exception\n{e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Info;
    impl QueryServerInfo for Info {
        fn server_port(&self) -> i32 {
            25565
        }
        fn server_name(&self) -> String {
            "A Minecraft Server".into()
        }
        fn max_players(&self) -> i32 {
            20
        }
        fn level_id_name(&self) -> String {
            "world".into()
        }
        fn player_count(&self) -> i32 {
            2
        }
        fn player_names(&self) -> Vec<String> {
            vec!["Steve".into(), "Alex".into()]
        }
        fn server_version(&self) -> String {
            "26.3".into()
        }
    }

    fn responder() -> Responder {
        Responder {
            info: Arc::new(Info),
            server_port: 25565,
            max_players: 20,
            server_name: "A Minecraft Server".into(),
            world_name: "world".into(),
            host_ip: "10.0.0.5".into(),
            valid_challenges: HashMap::new(),
            rules_response: NetworkDataOutputStream::new(MAX_PACKET_SIZE),
            last_rules_response: None,
            last_challenge_check: Instant::now(),
        }
    }

    #[test]
    fn handshake_status_and_rules() {
        let mut r = responder();
        let src: std::net::SocketAddr = "127.0.0.1:5000".parse().unwrap();
        // no challenge yet: a stat request is ignored
        let stat = |challenge: i32, padding: bool| {
            let mut p = vec![0xFE, 0xFD, 0, 1, 2, 3, 4];
            p.extend_from_slice(&challenge.to_be_bytes());
            if padding {
                p.extend_from_slice(&[0; 4]);
            }
            p
        };
        assert!(r.process_packet(&stat(0, false), src).is_none());
        let reply = r.process_packet(&[0xFE, 0xFD, 9, 1, 2, 3, 4], src).unwrap();
        assert_eq!(reply[0], 9);
        let token = r.valid_challenges[&src].challenge;
        assert!(token < 16_777_216);
        let basic = r.process_packet(&stat(token, false), src).unwrap();
        let mut expected = vec![0, 1, 2, 3, 4];
        for s in ["A Minecraft Server", "SMP", "world", "2", "20"] {
            expected.extend_from_slice(s.as_bytes());
            expected.push(0);
        }
        expected.extend_from_slice(&25565i16.to_le_bytes());
        expected.extend_from_slice(b"10.0.0.5\0");
        assert_eq!(basic, expected);
        let full = r.process_packet(&stat(token, true), src).unwrap();
        assert_eq!(&full[..5], [0, 1, 2, 3, 4]);
        let has = |needle: &[u8]| full.windows(needle.len()).any(|w| w == needle);
        assert!(has(b"splitnum\0\x80\0hostname\0A Minecraft Server\0gametype\0SMP\0game_id\0MINECRAFT\0version\x0026.3\0plugins\0\0map\0world\0numplayers\x002\0"));
        assert!(full.ends_with(b"\0\x01player_\0\0Steve\0Alex\0\0"));
    }
}
