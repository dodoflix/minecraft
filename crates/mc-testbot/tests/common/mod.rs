#![allow(dead_code)]
//! Scenario suite shared by the vanilla and (later) our-server integration tests.
use std::time::Duration;

pub mod dual;

use mc_core::direction::Direction;
use mc_core::pos::block_pos::BlockPos;
use mc_data::{Block, Item};
use mc_protocol::components::ItemStack;
use mc_protocol::packets::play::{ContainerInput, InteractionHand};
use mc_testbot::join::BotOptions;
use mc_testbot::{Bot, BotError};

pub const T: Duration = Duration::from_secs(30);

/// Console access to the server under test (the oracle script, or ours).
pub trait Console {
    fn cmd(&self, command: &str);
    fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String>;
}

impl Console for mc_testbot::harness::VanillaServer {
    fn cmd(&self, command: &str) {
        mc_testbot::harness::VanillaServer::cmd(self, command)
    }
    fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        mc_testbot::harness::VanillaServer::wait_log(self, regex, timeout_s)
    }
}

pub async fn join(host: &str, port: u16, name: &str) -> std::sync::Arc<Bot> {
    let mut o = BotOptions::new(host, port, name);
    o.timeout = Duration::from_secs(120);
    let bot = Bot::connect(o).await.expect("connect");
    bot.wait_joined(Duration::from_secs(60)).await.expect("login packet");
    bot.wait_ready(Duration::from_secs(90)).await.expect("player loaded");
    std::sync::Arc::new(bot)
}

fn pos_of(bot: &Bot) -> (i32, i32, i32) {
    let p = bot.position();
    (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
}

/// Join, survive `secs` seconds walking square loops; must not be rubber-banded (no extra teleports) or kicked.
pub async fn walk_scenario(bot: &Bot, secs: u64) {
    let start = bot.position();
    let teleports_after_join = bot.with_state(|s| s.teleports);
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    let mut travelled = 0.0;
    let mut last = start;
    let offsets = [(6.0, 0.0), (6.0, 6.0), (0.0, 6.0), (0.0, 0.0)];
    'outer: loop {
        for (dx, dz) in offsets {
            if std::time::Instant::now() >= end {
                break 'outer;
            }
            let r = bot.walk_to(start.x + dx, start.z + dz, 0.6, dx + dz > 8.0, Duration::from_secs(12)).await;
            match r {
                Ok(()) | Err(BotError::Timeout(_)) => {}
                Err(e) => panic!("walk failed: {e}\n{}", bot.log_tail(40)),
            }
            let p = bot.position();
            travelled += ((p.x - last.x).powi(2) + (p.z - last.z).powi(2)).sqrt();
            last = p;
        }
    }
    bot.stop();
    bot.assert_connected().unwrap_or_else(|e| panic!("{e}\n{}", bot.log_tail(40)));
    let teleports = bot.with_state(|s| s.teleports);
    assert_eq!(teleports, teleports_after_join, "rubber-banded: server teleported the bot while walking\n{}", bot.log_tail(40));
    assert!(travelled > 8.0, "bot barely moved: {travelled}");
}

/// Find a spot next to the bot with a solid block below and two blocks of air above.
fn placement_target(bot: &Bot) -> BlockPos {
    let (bx, by, bz) = pos_of(bot);
    let mut spots: Vec<(i32, i32)> = (-4i32..=4).flat_map(|x| (-4i32..=4).map(move |z| (x, z))).filter(|(x, z)| (1..=4).contains(&(x * x + z * z).isqrt())).collect();
    spots.sort_by_key(|(x, z)| (x - 2).abs() + z.abs());
    for (dx, dz) in spots {
        let (x, z) = (bx + dx, bz + dz);
        for y in (by - 3)..=(by + 3) {
            let below = bot.block_at(BlockPos::new(x, y - 1, z));
            let here = bot.block_at(BlockPos::new(x, y, z));
            let above = bot.block_at(BlockPos::new(x, y + 1, z));
            if let (Some(b), Some(h), Some(a)) = (below, here, above)
                && !b.block().is_air()
                && !b.can_be_replaced()
                && !b.collision_shape().is_empty()
                && b.get_fluid_state().data().is_empty
                && h.block().is_air()
                && a.block().is_air()
            {
                return BlockPos::new(x, y, z);
            }
        }
    }
    panic!("no free spot near the bot");
}

/// Place the held stack's block and break it again; `item` is put in hotbar slot 0 (creative: via set slot).
pub async fn place_and_break(bot: &Bot, creative: bool) {
    bot.select_hotbar(0);
    if creative {
        bot.creative_set_slot(36, ItemStack::with_count(Item::DIRT, 16));
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let target = placement_target(bot);
    let placed = bot.place_block(target, InteractionHand::MainHand, T).await.unwrap_or_else(|e| panic!("place: {e}\n{}", bot.log_tail(40)));
    assert_eq!(placed.block(), Block::DIRT, "placed block");
    bot.dig_block(target, Direction::Up, Duration::from_secs(40)).await.unwrap_or_else(|e| panic!("dig: {e}\n{}", bot.log_tail(40)));
    assert!(bot.block_at(target).is_some_and(|s| s.block().is_air()));
}

/// Chat + `/time query gametime` (26.3 has no `day` query) feedback.
pub async fn chat_and_command(bot: &Bot) {
    let since = bot.chat_len();
    bot.chat("hello from the testbot");
    bot.wait_chat("hello from the testbot", since, T).await.unwrap_or_else(|e| panic!("chat echo: {e}\n{}", bot.log_tail(40)));
    let fb = bot
        .command_expect("time query gametime", "game time is", T)
        .await
        .unwrap_or_else(|e| panic!("time query feedback: {e}\n{}", bot.log_tail(40)));
    println!("/time query gametime -> {fb}");
}

/// Opens the inventory, moves a stack between slots, checks the server's view through `data get`.
pub async fn inventory_move(bot: &Bot, name: &str) {
    bot.creative_set_slot(36, ItemStack::with_count(Item::STONE, 64));
    tokio::time::sleep(Duration::from_millis(300)).await;
    bot.move_item(36, 9);
    assert!(bot.slot(9).is_some_and(|s| s.is(Item::STONE) && s.count() == 64));
    assert!(bot.slot(36).is_some_and(|s| s.is_empty()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let fb = bot
        .command_expect(&format!("data get entity {name} Inventory"), "Slot: 9b", T)
        .await
        .unwrap_or_else(|e| panic!("server did not accept the move: {e}\n{}", bot.log_tail(40)));
    assert!(fb.contains("minecraft:stone"), "{fb}");
    bot.close_container();
    let _ = ContainerInput::Pickup;
}

/// Run one scenario on its own task so a failure does not hide the others; prints PASS/FAIL.
pub async fn scenario<F, Fut>(name: &str, bot: &std::sync::Arc<Bot>, f: F) -> bool
where
    F: FnOnce(std::sync::Arc<Bot>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    match tokio::spawn(f(bot.clone())).await {
        Ok(()) => {
            println!("SCENARIO PASS {name}");
            true
        }
        Err(e) => {
            println!("SCENARIO FAIL {name}: {e}");
            false
        }
    }
}

/// The full acceptance suite: walk, chat/command, creative place/break, inventory, survival place/break.
/// Returns the names of the failed scenarios.
pub async fn run_suite(bot: &std::sync::Arc<Bot>, console: &(dyn Console + Sync)) -> Vec<&'static str> {
    let mut failed = Vec::new();
    macro_rules! sc {
        ($name:literal, |$b:ident| $body:expr) => {
            if !scenario($name, bot, |$b| async move { $body.await }).await {
                failed.push($name);
            }
        };
    }
    sc!("walk_30s", |b| async move { walk_scenario(&b, 32).await });
    sc!("chat_and_command", |b| async move { chat_and_command(&b).await });
    sc!("creative_place_break", |b| async move { place_and_break(&b, true).await });
    sc!("inventory_move", |b| async move { inventory_move(&b, "Bot").await });
    console.cmd("gamemode survival Bot");
    console.cmd("give Bot minecraft:dirt 16");
    sc!("survival_place_break", |b| async move {
        b.wait_for(Duration::from_secs(20), |s| (!s.body.abilities.instabuild && !s.held_item().is_empty()).then_some(())).await
            .unwrap_or_else(|e| panic!("survival switch/give: {e}\n{}", b.log_tail(40)));
        place_and_break(&b, false).await
    });
    if let Some(r) = bot.disconnect_reason() {
        println!("SCENARIO FAIL still_connected: {r}");
        failed.push("still_connected");
    }
    failed
}
