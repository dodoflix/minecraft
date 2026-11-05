//! Handshake + status + legacy ping against our listener (mock server hooks).
extern crate mc_game_core as mc_game;
#[path = "../../mc-game/tests/net_common/mod.rs"]
mod net_common;
use net_common as common;

use common::*;
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::packets::handshake::ClientIntent;
use mc_protocol::packets::login::ClientboundLoginPacket;
use mc_protocol::packets::status::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn encode(p: ServerboundStatusPacket) -> Vec<u8> {
    let mut buf = Vec::new();
    p.encode_payload(&mut buf).unwrap();
    buf
}

#[tokio::test(flavor = "multi_thread")]
async fn status_request_then_ping() {
    let server = TestServer::start(MockHooks::new());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Status).await;
    write_payload(&mut c, &encode(ServerboundStatusPacket::StatusRequest(ServerboundStatusRequestPacket))).await;
    let ClientboundStatusPacket::StatusResponse(resp) = ClientboundStatusPacket::decode_payload(&read(&mut c).await).unwrap() else {
        panic!("expected status response")
    };
    assert_eq!(resp.status, **server.hooks.status.lock().as_ref().unwrap());
    write_payload(&mut c, &encode(ServerboundStatusPacket::PingRequest(ServerboundPingRequestPacket { time: 0x1234_5678_9abc }))).await;
    let ClientboundStatusPacket::PongResponse(pong) = ClientboundStatusPacket::decode_payload(&read(&mut c).await).unwrap() else {
        panic!("expected pong")
    };
    assert_eq!(pong.time, 0x1234_5678_9abc);
    expect_closed(&mut c).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn second_status_request_closes_the_connection() {
    let server = TestServer::start(MockHooks::new());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Status).await;
    let request = encode(ServerboundStatusPacket::StatusRequest(ServerboundStatusRequestPacket));
    write_payload(&mut c, &request).await;
    read(&mut c).await;
    write_payload(&mut c, &request).await;
    expect_closed(&mut c).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn status_is_ignored_before_the_server_is_ready_or_when_disabled() {
    let hooks = MockHooks::new();
    *hooks.status.lock() = None;
    let server = TestServer::start(hooks.clone());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Status).await;
    // `connection.disconnect(IGNORE_STATUS_REASON)` closes without a packet.
    expect_closed(&mut c).await;

    let hooks = MockHooks::new();
    hooks.settings.lock().replies_to_status = false;
    let server = TestServer::start(hooks);
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Status).await;
    expect_closed(&mut c).await;
}

async fn login_disconnect_text(addr: std::net::SocketAddr, protocol: i32, intent: ClientIntent) -> String {
    let mut c = connect(addr).await;
    handshake(&mut c, protocol, intent).await;
    let ClientboundLoginPacket::LoginDisconnect(d) = ClientboundLoginPacket::decode_payload(&read(&mut c).await).unwrap() else {
        panic!("expected login disconnect")
    };
    expect_closed(&mut c).await;
    d.reason.0.get_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn protocol_mismatch_is_rejected_with_vanilla_texts() {
    let server = TestServer::start(MockHooks::new());
    assert_eq!(login_disconnect_text(server.addr, 753, ClientIntent::Login).await, "Incompatible client! Please use 26.3");
    assert_eq!(login_disconnect_text(server.addr, 754, ClientIntent::Login).await, "Incompatible client! Please use 26.3");
    assert_eq!(login_disconnect_text(server.addr, PROTOCOL_VERSION + 1, ClientIntent::Login).await, "Incompatible client! Please use 26.3");
    assert_eq!(login_disconnect_text(server.addr, 10, ClientIntent::Transfer).await, "This server does not accept transfers");
    // transfers enabled: the protocol check applies like for login
    server.hooks.settings.lock().accepts_transfers = true;
    assert_eq!(login_disconnect_text(server.addr, 10, ClientIntent::Transfer).await, "Incompatible client! Please use 26.3");
}

#[tokio::test(flavor = "multi_thread")]
async fn garbage_handshake_is_closed() {
    let server = TestServer::start(MockHooks::new());
    // Unknown packet id 0x7f in the handshake state.
    let mut c = connect(server.addr).await;
    write_payload(&mut c, &[0x7f]).await;
    expect_closed(&mut c).await;
    // A frame length wider than 21 bits.
    let mut s = TcpStream::connect(server.addr).await.unwrap();
    s.write_all(&[0xff, 0xff, 0xff, 0xff, 0x01]).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut buf)).await.expect("closed");
}

fn legacy_utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

async fn legacy_exchange(addr: std::net::SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(request).await.unwrap();
    let mut buf = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut buf)).await.expect("closed").unwrap();
    buf
}

fn expected_legacy(body: &str) -> Vec<u8> {
    let mut out = vec![255];
    out.extend((body.encode_utf16().count() as u16).to_be_bytes());
    out.extend(legacy_utf16(body));
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_ping_forms() {
    let server = TestServer::start(MockHooks::new());
    assert_eq!(legacy_exchange(server.addr, &[0xfe]).await, expected_legacy("A Minecraft Server\u{a7}0\u{a7}20"));
    let v1 = expected_legacy("\u{a7}1\0127\026.3\0A Minecraft Server\00\020");
    assert_eq!(legacy_exchange(server.addr, &[0xfe, 0x01]).await, v1);
    // 1.6: FE 01 FA, "MC|PingHost", payload { protocol, host, port }
    let host = legacy_utf16("localhost");
    let mut payload = vec![78u8];
    payload.extend(9u16.to_be_bytes());
    payload.extend(&host);
    payload.extend(25565i32.to_be_bytes());
    let mut req = vec![0xfe, 0x01, 0xfa];
    req.extend(11u16.to_be_bytes());
    req.extend(legacy_utf16("MC|PingHost"));
    req.extend((payload.len() as u16).to_be_bytes());
    req.extend(payload);
    assert_eq!(legacy_exchange(server.addr, &req).await, v1);
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_ping_is_not_answered_when_status_replies_are_off() {
    let hooks = MockHooks::new();
    hooks.settings.lock().replies_to_status = false;
    let server = TestServer::start(hooks);
    // 0xFE is then just the first byte of a (not yet complete) frame length: no reply, connection stays open.
    let mut s = TcpStream::connect(server.addr).await.unwrap();
    s.write_all(&[0xfe]).await.unwrap();
    let mut buf = [0u8; 8];
    assert!(tokio::time::timeout(std::time::Duration::from_millis(300), s.read(&mut buf)).await.is_err());
}
