//! `RconThread` (listener) and `RconClient` (one connection), speaking the Source RCON protocol exactly as vanilla
//! does, quirks included: each `read` must contain exactly one whole packet, otherwise the connection is dropped.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::pkt_utils::{MAX_PACKET_SIZE, int_from_byte_array, string_from_byte_array};
use super::{GenericThread, RconCommandRunner};

const SERVERDATA_AUTH: i32 = 3;
const SERVERDATA_EXECCOMMAND: i32 = 2;
const SERVERDATA_RESPONSE_VALUE: i32 = 0;
const SERVERDATA_AUTH_RESPONSE: i32 = 2;
const SERVERDATA_AUTH_FAILURE: i32 = -1;

/// `RconClient`.
pub struct RconClient {
    thread: GenericThread,
    stream: TcpStream,
}

impl RconClient {
    /// Start serving `stream`. The thread is named `RCON Client /<peer ip> #<n>`.
    pub fn start(
        runner: Arc<dyn RconCommandRunner>,
        password: String,
        stream: TcpStream,
    ) -> Option<RconClient> {
        let peer = stream
            .peer_addr()
            .map(|a| format!("/{}", a.ip()))
            .unwrap_or_else(|_| "/?".to_owned());
        let name = format!("RCON Client {peer}");
        stream.set_read_timeout(None).ok()?;
        let worker_stream = stream.try_clone().ok()?;
        let mut thread = GenericThread::new(name.clone());
        let started =
            thread.start(move |running| serve(runner, &password, worker_stream, &running, &name));
        started.then_some(RconClient { thread, stream })
    }

    pub fn is_running(&self) -> bool {
        self.thread.is_running()
    }

    /// `stop`: close the socket (unblocking the read) and join.
    pub fn stop(&mut self) {
        self.thread.running_flag().store(false, Ordering::SeqCst);
        let _ = self.stream.shutdown(Shutdown::Both);
        self.thread.stop();
    }
}

fn send(stream: &mut TcpStream, request_id: i32, cmd: i32, text: &str) -> std::io::Result<()> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + 14);
    out.extend_from_slice(&(bytes.len() as i32 + 10).to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out.extend_from_slice(&cmd.to_le_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(&[0, 0]);
    stream.write_all(&out)
}

fn send_auth_failure(stream: &mut TcpStream) -> std::io::Result<()> {
    send(
        stream,
        SERVERDATA_AUTH_FAILURE,
        SERVERDATA_AUTH_RESPONSE,
        "",
    )
}

/// `sendCmdResponse`: split into chunks of 4096 UTF-16 code units; an empty response still sends one packet.
fn send_cmd_response(
    stream: &mut TcpStream,
    request_id: i32,
    response: &str,
) -> std::io::Result<()> {
    let units: Vec<u16> = response.encode_utf16().collect();
    let mut rest = &units[..];
    loop {
        let n = rest.len().min(4096);
        // Java `substring` happily splits surrogate pairs; the lossy conversion matches how they would be encoded
        send(
            stream,
            request_id,
            SERVERDATA_RESPONSE_VALUE,
            &String::from_utf16_lossy(&rest[..n]),
        )?;
        rest = &rest[n..];
        if rest.is_empty() {
            return Ok(());
        }
    }
}

fn serve(
    runner: Arc<dyn RconCommandRunner>,
    password: &str,
    mut stream: TcpStream,
    running: &AtomicBool,
    name: &str,
) {
    let mut authed = false;
    let mut buf = [0u8; MAX_PACKET_SIZE];
    let result: std::io::Result<()> = (|| {
        while running.load(Ordering::SeqCst) {
            let read = stream.read(&mut buf)?;
            if read < 10 {
                return Ok(());
            }
            let pktsize = int_from_byte_array(&buf, 0, read);
            if pktsize != read as i32 - 4 {
                return Ok(());
            }
            let request_id = int_from_byte_array(&buf, 4, read);
            let cmd = int_from_byte_array(&buf, 8, buf.len());
            let offset = 12;
            match cmd {
                SERVERDATA_EXECCOMMAND => {
                    if authed {
                        let Some(command) = string_from_byte_array(&buf, offset, read) else {
                            log::error!("Exception whilst parsing RCON input");
                            return Ok(());
                        };
                        send_cmd_response(&mut stream, request_id, &runner.run(&command))?;
                    } else {
                        send_auth_failure(&mut stream)?;
                    }
                }
                SERVERDATA_AUTH => {
                    let Some(given) = string_from_byte_array(&buf, offset, read) else {
                        log::error!("Exception whilst parsing RCON input");
                        return Ok(());
                    };
                    if !given.is_empty() && given == password {
                        authed = true;
                        send(&mut stream, request_id, SERVERDATA_AUTH_RESPONSE, "")?;
                    } else {
                        authed = false;
                        send_auth_failure(&mut stream)?;
                    }
                }
                other => send_cmd_response(
                    &mut stream,
                    request_id,
                    &format!("Unknown request {:x}", other),
                )?,
            }
        }
        Ok(())
    })();
    drop(result); // IOExceptions just end the connection
    let _ = stream.shutdown(Shutdown::Both);
    log::info!("Thread {name} shutting down");
    running.store(false, Ordering::SeqCst);
}

/// `RconThread`.
pub struct RconThread {
    thread: GenericThread,
    clients: Arc<parking_lot::Mutex<Vec<RconClient>>>,
}

impl RconThread {
    /// `RconThread.create`: `None` (after logging) for an invalid port, a missing password or a bind failure.
    pub fn create(
        runner: Arc<dyn RconCommandRunner>,
        server_ip: &str,
        port: i32,
        password: &str,
    ) -> Option<RconThread> {
        let server_ip = if server_ip.is_empty() {
            "0.0.0.0"
        } else {
            server_ip
        };
        if !(0 < port && port <= 65535) {
            log::warn!("Invalid rcon port {port} found in server.properties, rcon disabled!");
            return None;
        }
        if password.is_empty() {
            log::warn!("No rcon password set in server.properties, rcon disabled!");
            return None;
        }
        let bound = (server_ip, port as u16)
            .to_socket_addrs()
            .and_then(|mut a| {
                a.next()
                    .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, "no address"))
            })
            .and_then(TcpListener::bind)
            .and_then(|l| l.set_nonblocking(true).map(|()| l));
        let listener = match bound {
            Ok(l) => l,
            Err(e) => {
                log::warn!("Unable to initialise RCON on {server_ip}:{port}\n{e}");
                return None;
            }
        };
        let clients: Arc<parking_lot::Mutex<Vec<RconClient>>> = Arc::default();
        let mut thread = GenericThread::new("RCON Listener");
        let (worker_clients, password_owned) = (clients.clone(), password.to_owned());
        if !thread.start(move |running| {
            listen(
                &listener,
                &running,
                runner,
                &password_owned,
                &worker_clients,
            )
        }) {
            return None;
        }
        log::info!("RCON running on {server_ip}:{port}");
        Some(RconThread { thread, clients })
    }

    pub fn is_running(&self) -> bool {
        self.thread.is_running()
    }

    /// `stop`: stop listening, then stop every client.
    pub fn stop(&mut self) {
        self.thread.stop();
        for client in self.clients.lock().iter_mut() {
            if client.is_running() {
                client.stop();
            }
        }
        self.clients.lock().clear();
    }
}

fn listen(
    listener: &TcpListener,
    running: &AtomicBool,
    runner: Arc<dyn RconCommandRunner>,
    password: &str,
    clients: &parking_lot::Mutex<Vec<RconClient>>,
) {
    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                if let Some(client) = RconClient::start(runner.clone(), password.to_owned(), stream)
                {
                    clients.lock().push(client);
                }
                clients.lock().retain(RconClient::is_running);
            }
            // vanilla blocks in accept with a 500 ms timeout; polling is the std equivalent
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                clients.lock().retain(RconClient::is_running);
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                if running.load(Ordering::SeqCst) {
                    log::info!("IO exception: \n{e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    struct Echo;
    impl RconCommandRunner for Echo {
        fn run(&self, cmd: &str) -> String {
            if cmd == "long" {
                "x".repeat(5000)
            } else {
                format!("ran: {cmd}")
            }
        }
    }

    fn packet(id: i32, cmd: i32, body: &str) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(body.len() as i32 + 10).to_le_bytes());
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&cmd.to_le_bytes());
        p.extend_from_slice(body.as_bytes());
        p.extend_from_slice(&[0, 0]);
        p
    }

    fn read_packet(s: &mut TcpStream) -> (i32, i32, String) {
        let mut head = [0u8; 12];
        s.read_exact(&mut head).unwrap();
        let len = i32::from_le_bytes(head[0..4].try_into().unwrap()) as usize;
        let mut rest = vec![0u8; len - 8];
        s.read_exact(&mut rest).unwrap();
        let id = i32::from_le_bytes(head[4..8].try_into().unwrap());
        let ty = i32::from_le_bytes(head[8..12].try_into().unwrap());
        (
            id,
            ty,
            String::from_utf8(rest[..rest.len() - 2].to_vec()).unwrap(),
        )
    }

    #[test]
    fn auth_and_commands() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let mut rcon = RconThread::create(Arc::new(Echo), "127.0.0.1", port as i32, "secret")
            .expect("rcon starts");
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(&packet(7, SERVERDATA_EXECCOMMAND, "say hi"))
            .unwrap();
        assert_eq!(read_packet(&mut c), (-1, 2, String::new()));
        c.write_all(&packet(8, SERVERDATA_AUTH, "wrong")).unwrap();
        assert_eq!(read_packet(&mut c), (-1, 2, String::new()));
        c.write_all(&packet(9, SERVERDATA_AUTH, "secret")).unwrap();
        assert_eq!(read_packet(&mut c), (9, 2, String::new()));
        c.write_all(&packet(10, SERVERDATA_EXECCOMMAND, "list"))
            .unwrap();
        assert_eq!(read_packet(&mut c), (10, 0, "ran: list".to_owned()));
        c.write_all(&packet(11, SERVERDATA_EXECCOMMAND, "long"))
            .unwrap();
        assert_eq!(read_packet(&mut c).2.len(), 4096);
        assert_eq!(read_packet(&mut c).2.len(), 904);
        c.write_all(&packet(12, 99, "")).unwrap();
        assert_eq!(
            read_packet(&mut c),
            (12, 0, "Unknown request 63".to_owned())
        );
        rcon.stop();
    }

    #[test]
    fn create_validates_settings() {
        assert!(RconThread::create(Arc::new(Echo), "", 0, "pw").is_none());
        assert!(RconThread::create(Arc::new(Echo), "", 70000, "pw").is_none());
        assert!(RconThread::create(Arc::new(Echo), "", 25575, "").is_none());
    }
}
