//! `QueryThreadGs4`: the GameSpy-4 UDP query listener (`enable-query`, `query.port`).

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use mc_core::java_string::decode_utf8;
use rand::RngExt;

use super::GenericThread;
use super::pkt_utils::{
    MAX_PACKET_SIZE, NetworkDataOutputStream, int_from_network_byte_array, to_hex_string,
};

const GAME_TYPE: &str = "SMP";
const GAME_ID: &str = "MINECRAFT";
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

/// A handshake's token. Vanilla's `pruneChallenges` runs every 30 s but compares the challenge's epoch milliseconds
/// (`new Date().getTime()`) with `Util.getMillis()` (the monotonic clock), which is far smaller, so no challenge is
/// ever removed; neither does this port.
struct RequestChallenge {
    challenge: i32,
    ident_bytes: [u8; 4],
    challenge_bytes: Vec<u8>,
}

impl RequestChallenge {
    /// `buffer` is the whole receive buffer, not the datagram: a type-9 datagram shorter than 7 bytes takes the rest
    /// of its ident from the bytes the previous datagram left there.
    fn new(buffer: &[u8]) -> RequestChallenge {
        let ident_bytes = [buffer[3], buffer[4], buffer[5], buffer[6]];
        let ident = decode_utf8(&ident_bytes);
        let challenge = rand::rng().random_range(0..16_777_216);
        RequestChallenge {
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

    /// `processPacket`: the datagram to send back, if any. The datagram is `buffer[..len]`; `buffer` is the receive
    /// buffer that lives as long as the listener.
    fn process_packet(
        &mut self,
        buffer: &[u8],
        len: usize,
        src: std::net::SocketAddr,
    ) -> Option<Vec<u8>> {
        let data = &buffer[..len];
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
                    let challenge = RequestChallenge::new(buffer);
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
}

/// `InetAddress.getLocalHost().getHostAddress()`: the host name's first IPv4 address (Java prefers IPv4 unless
/// `java.net.preferIPv6Addresses` is set), else its first IPv6 address.
fn local_host_address() -> Option<String> {
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())?;
    java_host_address((name.trim(), 0).to_socket_addrs().ok()?)
}

/// The address `getLocalHost` picks from the resolver's answers, as `getHostAddress` prints it.
fn java_host_address(addrs: impl Iterator<Item = SocketAddr>) -> Option<String> {
    let addrs: Vec<SocketAddr> = addrs.collect();
    let first = addrs.iter().find(|a| a.is_ipv4()).or(addrs.first())?;
    Some(match first {
        // Inet6Address: eight hex groups, no zero compression, `%scope` when the resolver gave a scope id
        SocketAddr::V6(v6) => {
            let groups = v6.ip().segments().map(|g| format!("{g:x}")).join(":");
            match v6.scope_id() {
                0 => groups,
                scope => format!("{groups}%{scope}"),
            }
        }
        v4 => v4.ip().to_string(),
    })
}

/// `initSocket`: bind `server_ip:port` with a 500 ms receive timeout.
fn init_socket(server_ip: &str, port: u16) -> Option<UdpSocket> {
    match (server_ip, port)
        .to_socket_addrs()
        .and_then(|mut a| a.next().ok_or_else(|| std::io::Error::other("no address")))
        .and_then(UdpSocket::bind)
        .and_then(|s| {
            s.set_read_timeout(Some(Duration::from_millis(500)))
                .map(|()| s)
        }) {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!("Unable to initialise query system on {server_ip}:{port}\n{e}");
            None
        }
    }
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
        let socket = init_socket(&server_ip, query_port as u16)?;
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
        };
        let mut thread = GenericThread::new("Query Listener");
        let started = thread.start(move |running| {
            log::info!("Query running on {server_ip}:{query_port}");
            run(
                socket,
                &running,
                &mut responder,
                &server_ip,
                query_port as u16,
            );
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

fn run(
    mut socket: UdpSocket,
    running: &AtomicBool,
    responder: &mut Responder,
    server_ip: &str,
    port: u16,
) {
    // One buffer for the listener's lifetime, like vanilla's `buffer`: a datagram only overwrites its own length.
    let mut buffer = [0u8; MAX_PACKET_SIZE];
    while running.load(Ordering::SeqCst) {
        // vanilla's `catch (IOException)` covers the reply too
        let result = socket.recv_from(&mut buffer).and_then(|(len, src)| {
            match responder.process_packet(&buffer, len, src) {
                Some(reply) => socket.send_to(&reply, src).map(drop),
                None => Ok(()),
            }
        });
        match result {
            Ok(()) => {}
            // the receive timeout, ICMP port unreachable (vanilla's `PortUnreachableException`), and a signal (the JDK
            // retries those itself)
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => {
                recover_socket_error(&mut socket, running, &e, || init_socket(server_ip, port))
            }
        }
    }
}

/// `recoverSocketError`: bind the socket again while the old one is still open (so, like vanilla, this fails for a
/// socket that is merely broken on the same address) and stop the listener when that fails.
fn recover_socket_error(
    socket: &mut UdpSocket,
    running: &AtomicBool,
    e: &std::io::Error,
    init: impl FnOnce() -> Option<UdpSocket>,
) {
    if !running.load(Ordering::SeqCst) {
        return;
    }
    log::warn!("Unexpected exception\n{e}");
    match init() {
        Some(new) => *socket = new,
        None => {
            log::error!("Failed to recover from exception, shutting down!");
            running.store(false, Ordering::SeqCst);
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
        }
    }

    /// Feeds `bytes` through a receive buffer the way `run` does: only the datagram's own bytes are overwritten.
    fn receive(
        r: &mut Responder,
        buffer: &mut [u8; MAX_PACKET_SIZE],
        bytes: &[u8],
        src: std::net::SocketAddr,
    ) -> Option<Vec<u8>> {
        buffer[..bytes.len()].copy_from_slice(bytes);
        r.process_packet(buffer, bytes.len(), src)
    }

    const SRC: &str = "127.0.0.1:5000";

    #[test]
    fn handshake_status_and_rules() {
        let mut r = responder();
        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let src: std::net::SocketAddr = SRC.parse().unwrap();
        // no challenge yet: a stat request is ignored
        let stat = |challenge: i32, padding: bool| {
            let mut p = vec![0xFE, 0xFD, 0, 1, 2, 3, 4];
            p.extend_from_slice(&challenge.to_be_bytes());
            if padding {
                p.extend_from_slice(&[0; 4]);
            }
            p
        };
        assert!(receive(&mut r, &mut buffer, &stat(0, false), src).is_none());
        let reply = receive(&mut r, &mut buffer, &[0xFE, 0xFD, 9, 1, 2, 3, 4], src).unwrap();
        assert_eq!(reply[0], 9);
        let token = r.valid_challenges[&src].challenge;
        assert!(token < 16_777_216);
        let basic = receive(&mut r, &mut buffer, &stat(token, false), src).unwrap();
        let mut expected = vec![0, 1, 2, 3, 4];
        for s in ["A Minecraft Server", "SMP", "world", "2", "20"] {
            expected.extend_from_slice(s.as_bytes());
            expected.push(0);
        }
        expected.extend_from_slice(&25565i16.to_le_bytes());
        expected.extend_from_slice(b"10.0.0.5\0");
        assert_eq!(basic, expected);
        let full = receive(&mut r, &mut buffer, &stat(token, true), src).unwrap();
        assert_eq!(&full[..5], [0, 1, 2, 3, 4]);
        let has = |needle: &[u8]| full.windows(needle.len()).any(|w| w == needle);
        assert!(has(b"splitnum\0\x80\0hostname\0A Minecraft Server\0gametype\0SMP\0game_id\0MINECRAFT\0version\x0026.3\0plugins\0\0map\0world\0numplayers\x002\0"));
        assert!(full.ends_with(b"\0\x01player_\0\0Steve\0Alex\0\0"));
    }

    /// The handshake reply: `\t`, the ident as Java decodes it, the decimal token, NUL.
    fn ident_and_token(reply: &[u8]) -> (String, i32) {
        let text = String::from_utf8(reply[1..reply.len() - 1].to_vec()).unwrap();
        let digits = text.chars().rev().take_while(char::is_ascii_digit).count();
        let (ident, token) = text.split_at(text.len() - digits);
        (ident.to_owned(), token.parse().unwrap())
    }

    #[test]
    fn short_handshake_reads_the_previous_datagrams_bytes() {
        let mut r = responder();
        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let src: std::net::SocketAddr = SRC.parse().unwrap();
        // nothing was received before: the ident is four NULs
        let first = receive(&mut r, &mut buffer, &[0xFE, 0xFD, 9], src).unwrap();
        assert_eq!(ident_and_token(&first).0, "\0\0\0\0");
        // a long datagram of another type leaves `WXYZ` at 3..7 (no reply for type 7)
        let long = [&[0xFE, 0xFD, 7][..], b"WXYZ", &[0; 20]].concat();
        assert!(receive(&mut r, &mut buffer, &long, src).is_none());
        // 3..6 bytes: the ident takes what the datagram does not overwrite
        for (datagram, ident) in [
            (&[0xFE, 0xFD, 9][..], "WXYZ"),
            (&[0xFE, 0xFD, 9, b'a'][..], "aXYZ"),
            (&[0xFE, 0xFD, 9, b'a', b'b'][..], "abYZ"),
            (&[0xFE, 0xFD, 9, b'a', b'b', b'c'][..], "abcZ"),
        ] {
            let reply = receive(&mut r, &mut buffer, datagram, src).unwrap();
            assert_eq!(ident_and_token(&reply).0, ident);
            assert_eq!(r.valid_challenges[&src].ident_bytes, ident.as_bytes());
        }
    }

    #[test]
    fn ident_is_decoded_like_java() {
        let mut r = responder();
        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let src: std::net::SocketAddr = SRC.parse().unwrap();
        // `ed a0 80` is an encoded surrogate: Java makes one U+FFFD of it, `from_utf8_lossy` three
        let reply = receive(
            &mut r,
            &mut buffer,
            &[0xFE, 0xFD, 9, 0xED, 0xA0, 0x80, b'k'],
            src,
        )
        .unwrap();
        assert_eq!(ident_and_token(&reply).0, "\u{FFFD}k");
        // the stored ident bytes stay raw
        assert_eq!(
            r.valid_challenges[&src].ident_bytes,
            [0xED, 0xA0, 0x80, b'k']
        );
    }

    #[test]
    fn local_host_address_prefers_ipv4() {
        let addr = |s: &str| (s, 0).to_socket_addrs().unwrap().next().unwrap();
        let pick = |addrs: &[&str]| java_host_address(addrs.iter().map(|a| addr(a)));
        assert_eq!(pick(&["::1", "127.0.1.1"]).as_deref(), Some("127.0.1.1"));
        assert_eq!(pick(&["10.0.0.2", "10.0.0.3"]).as_deref(), Some("10.0.0.2"));
        // Inet6Address.getHostAddress: eight groups, no compression
        assert_eq!(pick(&["::1"]).as_deref(), Some("0:0:0:0:0:0:0:1"));
        assert_eq!(
            pick(&["fe80::a:1", "2001:db8::1"]).as_deref(),
            Some("fe80:0:0:0:0:0:a:1")
        );
        assert_eq!(pick(&[]), None);
        let scoped = SocketAddr::V6(std::net::SocketAddrV6::new(
            "fe80::1".parse().unwrap(),
            0,
            0,
            2,
        ));
        assert_eq!(
            java_host_address([scoped].into_iter()).as_deref(),
            Some("fe80:0:0:0:0:0:0:1%2")
        );
    }

    fn bound() -> UdpSocket {
        UdpSocket::bind("127.0.0.1:0").unwrap()
    }

    #[test]
    fn socket_error_rebinds_or_shuts_the_listener_down() {
        let running = AtomicBool::new(true);
        let err = std::io::Error::other("boom");
        let mut socket = bound();
        // the replacement socket is installed and the listener keeps running
        let fresh = bound();
        let fresh_addr = fresh.local_addr().unwrap();
        recover_socket_error(&mut socket, &running, &err, || Some(fresh));
        assert!(running.load(Ordering::SeqCst));
        assert_eq!(socket.local_addr().unwrap(), fresh_addr);
        // a failed re-bind stops it and keeps the old socket for the final close
        let mut socket = bound();
        let kept = socket.local_addr().unwrap();
        recover_socket_error(&mut socket, &running, &err, || None);
        assert!(!running.load(Ordering::SeqCst));
        assert_eq!(socket.local_addr().unwrap(), kept);
        // once stopped (by `stop`) an error does nothing
        recover_socket_error(&mut socket, &running, &err, || panic!("must not re-bind"));
    }

    #[test]
    fn rebinding_over_the_open_socket_fails_like_vanilla() {
        let socket = bound();
        let port = socket.local_addr().unwrap().port();
        assert!(init_socket("127.0.0.1", port).is_none());
        drop(socket);
        assert!(init_socket("127.0.0.1", port).is_some());
    }
}
