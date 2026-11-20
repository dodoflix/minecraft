//! A GameSpy-4 query client: one UDP socket with a fixed source port, for observing `enable-query` on a server.

use std::io::ErrorKind;
use std::net::UdpSocket;
use std::time::Duration;

/// How long [`QueryClient::send`] waits for an answer before it counts as none.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(1);

/// What a datagram got back.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// The reply datagram.
    Packet(Vec<u8>),
    /// Nothing within the timeout: the server dropped the datagram.
    Silent,
    /// The host answered with ICMP port unreachable: nothing listens on the port.
    Refused,
}

/// A UDP socket connected to `127.0.0.1:port` (connected, so that a closed port is observable).
pub struct QueryClient {
    socket: UdpSocket,
}

impl QueryClient {
    /// A client with a source port chosen by the OS.
    pub fn connect(port: u16) -> QueryClient {
        Self::connect_from(0, port)
    }

    /// A client with this source port (0: any); the query challenge table is keyed by source address.
    pub fn connect_from(source_port: u16, port: u16) -> QueryClient {
        let socket = UdpSocket::bind(("127.0.0.1", source_port)).expect("bind the query client");
        socket
            .connect(("127.0.0.1", port))
            .expect("connect the query client");
        QueryClient { socket }
    }

    /// The source port the server sees.
    pub fn source_port(&self) -> u16 {
        self.socket.local_addr().expect("local address").port()
    }

    /// Sends one datagram (any length, including zero) without waiting for an answer.
    pub fn send_only(&self, datagram: &[u8]) {
        // a closed port reports on the next `recv`, not here
        let _ = self.socket.send(datagram);
    }

    /// The next datagram within `timeout`.
    pub fn recv(&self, timeout: Duration) -> Answer {
        self.socket
            .set_read_timeout(Some(timeout))
            .expect("set the read timeout");
        let mut buf = vec![0u8; 4096];
        match self.socket.recv(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                Answer::Packet(buf)
            }
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => Answer::Refused,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Answer::Silent
            }
            Err(e) => panic!("query client receive: {e}"),
        }
    }

    /// Sends one datagram and waits up to one second for the answer.
    pub fn send(&self, datagram: &[u8]) -> Answer {
        self.send_only(datagram);
        self.recv(ANSWER_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_silence_and_refusal() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let client = QueryClient::connect(port);
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        // echoed
        client.send_only(&[1, 2, 3]);
        let mut buf = [0u8; 16];
        let (n, from) = server.recv_from(&mut buf).unwrap();
        assert_eq!((n, from.port()), (3, client.source_port()));
        server.send_to(&buf[..n], from).unwrap();
        assert_eq!(
            client.recv(Duration::from_secs(5)),
            Answer::Packet(vec![1, 2, 3])
        );
        // zero-length datagrams are sent
        client.send_only(&[]);
        assert_eq!(server.recv_from(&mut buf).unwrap().0, 0);
        // nobody answers
        assert_eq!(client.recv(Duration::from_millis(50)), Answer::Silent);
        // the port closes
        drop(server);
        assert_eq!(client.send(b"x"), Answer::Refused);
    }
}
