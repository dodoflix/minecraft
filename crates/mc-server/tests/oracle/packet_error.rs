//! A serverbound handler that throws on the main thread (a negative block-change sequence) is logged and suppressed:
//! the player stays connected. One that throws on the network thread (an unsolicited configuration acknowledgement)
//! kicks. Both on the real vanilla server and on ours.
use crate::common::*;
use mc_protocol::packets::play::{
    InteractionHand, ServerboundConfigurationAcknowledgedPacket, ServerboundPlayPacket,
    ServerboundUseItemPacket,
};
use mc_testbot::BotError;

async fn bad_sequence_keeps_the_player(server: &(dyn Console + Sync), port: u16) {
    let bot = join("127.0.0.1", port, "Bot").await;
    bot.send(ServerboundPlayPacket::UseItem(ServerboundUseItemPacket {
        hand: InteractionHand::MainHand,
        sequence: -1,
        y_rot: 0.0,
        x_rot: 0.0,
    }));
    server
        .wait_log("Failed to handle packet .*suppressing error", 20)
        .expect("handler error is logged and suppressed");
    bot.wait_ticks(40).await.expect("still ticking");
    let reason = bot.disconnect_reason();
    bot.disconnect().await;
    assert_eq!(reason, None, "the player must stay connected");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn packet_error_keeps_player_on_vanilla() {
    let server = mc_testbot::harness::vanilla(12345, "creative", &[])
        .start()
        .expect("start vanilla");
    bad_sequence_keeps_the_player(&server, server.port).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn packet_error_keeps_player_on_our_server() {
    let server = ours(12345, "creative", &[])
        .start()
        .expect("start our server");
    bad_sequence_keeps_the_player(&server, server.port).await;
}

async fn unsolicited_config_ack_kicks(port: u16) {
    let bot = join("127.0.0.1", port, "Bot").await;
    bot.send(ServerboundPlayPacket::ConfigurationAcknowledged(
        ServerboundConfigurationAcknowledgedPacket,
    ));
    match bot.wait_for(T, |_| None::<()>).await {
        Err(BotError::Disconnected(reason)) => assert!(
            reason.contains("Client acknowledged config, but none was requested"),
            "{reason}"
        ),
        other => panic!("expected a disconnect, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn unsolicited_config_ack_kicks_on_vanilla() {
    let server = mc_testbot::harness::vanilla(12345, "creative", &[])
        .start()
        .expect("start vanilla");
    unsolicited_config_ack_kicks(server.port).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn unsolicited_config_ack_kicks_on_our_server() {
    let server = ours(12345, "creative", &[])
        .start()
        .expect("start our server");
    unsolicited_config_ack_kicks(server.port).await;
}
