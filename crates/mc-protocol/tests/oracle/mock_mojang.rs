//! The vanilla server in online mode with its Mojang services pointed at the mock (`mc_testkit::MockMojang`): a raw
//! client that joined through the mock's session endpoint is verified by the server's own `hasJoined` request, one that
//! did not is refused. Needs java, `ref/server.jar` and `MC_ACCEPT_EULA=1`.

use crate::common::*;
use mc_protocol::auth::{digest_server_id, encrypt_with_public_key, generate_secret};
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::{handshake::ClientIntent, login::*};
use mc_testkit::{MockMojang, offline_uuid};
use uuid::Uuid;

fn online_server(mock: &MockMojang) -> mc_testkit::Server {
    mc_testkit::Server::vanilla()
        .properties([
            "online-mode=true",
            "white-list=false",
            "enforce-whitelist=false",
            "generate-structures=false",
            "spawn-protection=0",
            "max-tick-time=-1",
            "view-distance=2",
            "simulation-distance=2",
            "rate-limit=0",
        ])
        .mojang(mock)
        .start()
        .expect("start the vanilla server")
}

/// Handshake, `Hello` and `Key` with `name`; `join` is called with the server hash before the key is sent. Returns the
/// first packet after encryption (or the compression packet's successor).
async fn login(port: u16, name: &str, join: impl AsyncFnOnce(&str)) -> Result<GameProfile, String> {
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Login).await;
    send!(
        c,
        ServerboundLoginPacket::Hello(ServerboundHelloPacket {
            name: name.into(),
            profile_id: Uuid::nil()
        })
    );
    let ClientboundLoginPacket::Hello(hello) =
        ClientboundLoginPacket::decode_payload(&read(&mut c).await).unwrap()
    else {
        panic!("expected hello")
    };
    assert!(hello.should_authenticate);
    let secret = generate_secret();
    join(&digest_server_id(
        &hello.server_id,
        &secret,
        &hello.public_key,
    ))
    .await;
    send!(
        c,
        ServerboundLoginPacket::Key(ServerboundKeyPacket {
            keybytes: encrypt_with_public_key(&hello.public_key, &secret).unwrap(),
            encrypted_challenge: encrypt_with_public_key(&hello.public_key, &hello.challenge)
                .unwrap(),
        })
    );
    c.enable_encryption(secret);
    loop {
        match ClientboundLoginPacket::decode_payload(&read(&mut c).await).unwrap() {
            ClientboundLoginPacket::LoginCompression(p) => {
                c.set_compression(p.compression_threshold, false)
            }
            ClientboundLoginPacket::LoginFinished(p) => return Ok(p.game_profile),
            ClientboundLoginPacket::LoginDisconnect(d) => return Err(d.reason.0.get_string()),
            other => panic!("unexpected login packet {other:?}"),
        }
    }
}

/// What the client's `joinServer` does: POST the join to the session endpoint named by the discovery document.
async fn join_server(mock: &MockMojang, name: &str, server_hash: &str) {
    let http = reqwest::Client::new();
    let discovery: serde_json::Value = http
        .get(mock.discovery_url())
        .send()
        .await
        .expect("fetch the discovery document")
        .json()
        .await
        .expect("discovery json");
    let url = discovery["discovery"]["session"]["endpoints"]["join"]["uri"]
        .as_str()
        .expect("session.join endpoint");
    let body = serde_json::json!({
        "accessToken": name,
        "selectedProfile": offline_uuid(name).simple().to_string(),
        "serverId": server_hash,
    });
    let status = http
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("post the join")
        .status();
    assert_eq!(status, 204);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn a_client_that_joined_through_the_mock_is_verified_by_vanilla() {
    let mock = MockMojang::start().expect("start the mock");
    let server = online_server(&mock);
    let name = "MockBot";
    let mut hash = String::new();
    let profile = login(server.port, name, async |h| {
        h.clone_into(&mut hash);
        join_server(&mock, name, h).await;
    })
    .await
    .expect("login finished");
    assert_eq!(profile.name, name);
    assert_eq!(profile.id, offline_uuid(name));
    let verify = mock
        .requests()
        .into_iter()
        .find(|r| r.target.starts_with("/session/minecraft/hasJoined?"))
        .expect("the server asked the mock to verify the join");
    assert!(
        verify.target.contains("username=MockBot"),
        "{}",
        verify.target
    );
    assert!(
        verify.target.contains(&format!("serverId={hash}")),
        "{}",
        verify.target
    );
    assert!(
        server
            .wait_log(
                &format!("UUID of player {name} is {}", offline_uuid(name)),
                10
            )
            .is_some(),
        "{}",
        server.log()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn a_client_that_never_joined_is_refused_by_vanilla() {
    let mock = MockMojang::start().expect("start the mock");
    let server = online_server(&mock);
    let reason = login(server.port, "Stranger", async |_| {})
        .await
        .expect_err("a login without a join");
    assert_eq!(reason, "Failed to verify username!");
    assert!(
        mock.requests()
            .iter()
            .any(|r| r.target.starts_with("/session/minecraft/hasJoined?")),
        "the server never reached the mock"
    );
}
