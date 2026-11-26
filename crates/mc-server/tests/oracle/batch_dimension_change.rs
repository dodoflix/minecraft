//! A chat command that moves the player to another dimension runs before the main-thread packets of its tick's batch; those
//! packets are then handled for the player in the new dimension, by the same connection.
//!
//! One batch: `set_carried_item 5` (main thread), `execute in the_nether run tp @s` and a query of the selected slot
//! (both run first, in this order). The query is answered in the nether and still reads slot 0, which proves that the
//! three packets met in one batch and the slot packet was handled after the dimension change; the slot read later is 5,
//! which proves that the packet was then handled for the player in the new dimension. The same script runs against the
//! vanilla server and ours.
use std::time::Duration;

use crate::common::*;
use mc_testbot::Bot;

const PROPS: &[&str] = &[
    "level-type=minecraft:flat",
    "generate-structures=false",
    "spawn-monsters=false",
    "spawn-animals=false",
];
const QUERY: &str = "data get entity @s SelectedItemSlot";

/// The slot a `data get` answer names.
fn slot_of(answer: &str) -> i32 {
    answer
        .rsplit(": ")
        .next()
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no slot in `{answer}`"))
}

async fn dimension_is(bot: &Bot, name: &str) {
    bot.wait_for(Duration::from_secs(60), |s| {
        (s.dimension.to_string() == name).then_some(())
    })
    .await
    .unwrap_or_else(|e| panic!("never arrived in {name}: {e}"));
}

/// The selected slot, asked until the answer is `want` (the slot packet is handled at the start of a later tick than a
/// command, so an early answer may still show the old slot).
async fn slot_settles_at(bot: &Bot, want: i32) {
    let deadline = std::time::Instant::now() + T;
    loop {
        let answer = bot
            .command_expect(QUERY, "entity data: ", T)
            .await
            .expect("the query");
        if slot_of(&answer) == want {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the slot never became {want}: `{answer}`"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn scenario(bot: &Bot) {
    for _ in 0..20 {
        // back to the overworld with slot 0 selected
        bot.command("execute in minecraft:overworld run tp @s 0 -60 0");
        dimension_is(bot, "minecraft:overworld").await;
        bot.select_hotbar(0);
        slot_settles_at(bot, 0).await;

        let since = bot.chat_len();
        bot.select_hotbar(5);
        bot.command("execute in minecraft:the_nether run tp @s 0 100 0");
        bot.command(QUERY);
        let in_batch = bot
            .wait_chat("entity data: ", since, T)
            .await
            .expect("the query");
        dimension_is(bot, "minecraft:the_nether").await;
        slot_settles_at(bot, 5).await;
        // the query ran after the dimension change and still read slot 0: the slot packet was handled later, by the
        // player in the new dimension
        if slot_of(&in_batch) == 0 {
            return;
        }
    }
    panic!("the three packets never arrived in one batch");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn batch_dimension_change_vanilla() {
    let server = mc_testbot::harness::vanilla(12345, "creative", PROPS)
        .start()
        .expect("start vanilla");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("Made Bot a server operator", 30);
    scenario(&bot).await;
    bot.disconnect().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn batch_dimension_change_ours() {
    let server = ours(12345, "creative", PROPS)
        .start()
        .expect("start our server");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("Made Bot a server operator", 30);
    scenario(&bot).await;
    bot.disconnect().await;
}
