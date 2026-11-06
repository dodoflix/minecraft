//! Starts the real vanilla server (`ref/server.jar`, via `mc-testkit`) and offers raw-connection helpers.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]
use std::time::Duration;

use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::net::Connection;
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::handshake::*;
use tokio::net::TcpStream;
use tokio::time::timeout;

pub fn start_vanilla(online_mode: bool) -> mc_testkit::Server {
    mc_testkit::Server::vanilla()
        .properties([
            format!("online-mode={online_mode}").as_str(),
            "generate-structures=false",
            "white-list=false",
            "enforce-whitelist=false",
            "spawn-protection=0",
            "max-tick-time=-1",
            "view-distance=2",
            "simulation-distance=2",
            "enable-rcon=false",
            "enable-query=false",
            "rate-limit=0",
        ])
        .start()
        .expect("start the vanilla server")
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
    timeout(Duration::from_secs(30), conn.read_payload())
        .await
        .expect("read timeout")
        .expect("read")
}

pub fn log_id(state: ConnectionState, payload: &[u8]) {
    let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
    println!(
        "<- {} clientbound id={id} {}",
        state.id(),
        packet_name(state, PacketFlow::Clientbound, id).unwrap_or("?")
    );
}
