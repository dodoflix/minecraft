//! `--demo` and `--bonusChest`, observed through the same scripts on the vanilla server and on OUR server.
//!
//! `timeline`     the demo events and messages a player receives, with the game time held (`/tick freeze`) and moved to
//!                the boundaries of every event with `/tick sprint`: the intro after 21 player ticks, the day-one hints
//!                at 100, 175 and 250, the message of each day, the warning on day five, the end of the demo at 120 500
//!                ticks and, after it, what a block break and an item use answer (the reminder every 101 ticks).
//! `level_data`   the `level.dat` of the demo world (survival, `North Carolina`, bonus chest and structures on).
//! `bonus_chest`  the chest `--bonusChest` places near the world spawn: the chunk, the block entity keys, the chest's
//!                surroundings, and the contents that a loot seed set on both servers rolls (the position inside the
//!                chunk and the loot seed come from the level's random, which vanilla's jar seeds from the clock; the
//!                seeded scenario `feat_bonus_chest_edge` of `mc-game` compares exact columns, loot seeds and the torches
//!                across chunk edges).
//!
//! `demo_dual_vanilla` runs the scenarios against the real vanilla server and checks them against the committed
//! observations in `tests/fixtures/demo_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them; `DEMO_DUAL_ONLY=name` runs
//! one scenario). `demo_dual_ours` runs them against `mc-server`.
use std::time::{Duration, Instant};

use crate::common::dual::PROPS;
use crate::common::world_diff::{list_files, read_dat, read_region};
use crate::kick_dual::{Case, Rec, case, compare_ours, record_vanilla};
use mc_core::direction::Direction;
use mc_core::pos::block_pos::BlockPos;
use mc_nbt::Tag;
use mc_protocol::packets::play::InteractionHand;
use mc_testbot::Bot;
use mc_testkit::Server;

const DIR: &str = "demo_dual";
const ONLY: &str = "DEMO_DUAL_ONLY";

const CASES: &[Case] = &[
    // sprinting runs thousands of ticks between two movement packets of the bot: the floating check would kick it
    case!(
        timeline,
        &["allow-flight=true"],
        1,
        None,
        false,
        &["--demo"]
    ),
    case!(level_data, PROPS, 1, None, false, &["--demo"]),
    case!(
        bonus_chest,
        &["level-type=minecraft:normal"],
        1,
        None,
        false,
        &["--bonusChest"]
    ),
];

/// Waits until `count()` exceeds `before`.
fn wait_for_more(what: &str, before: usize, count: impl Fn() -> usize) {
    let deadline = Instant::now() + Duration::from_secs(600);
    while count() <= before {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn log_count(server: &Server, needle: &str) -> usize {
    server.log().lines().filter(|l| l.contains(needle)).count()
}

/// `/time query gametime` on the console.
fn game_time(server: &Server) -> i64 {
    const NEEDLE: &str = "The game time is ";
    let before = log_count(server, NEEDLE);
    server.cmd("time query gametime");
    wait_for_more("the game time", before, || log_count(server, NEEDLE));
    let log = server.log();
    let line = log
        .lines()
        .rev()
        .find(|l| l.contains(NEEDLE))
        .expect("the game time line");
    line.split_once(NEEDLE)
        .and_then(|(_, n)| n.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("no game time in {line}"))
}

/// `/tick sprint ticks` on the console, until the report.
fn sprint(server: &Server, ticks: i64) {
    const NEEDLE: &str = "Sprint completed";
    let before = log_count(server, NEEDLE);
    server.cmd(&format!("tick sprint {ticks}"));
    wait_for_more("the end of the sprint", before, || {
        log_count(server, NEEDLE)
    });
}

/// Sprints the (frozen) game time to about `target` (a sprint ends a tick or two late) in chunks, letting the bot read
/// what each chunk sent; returns the time reached.
async fn advance_to(server: &Server, bot: &Bot, target: i64) -> i64 {
    loop {
        let now = game_time(server);
        if now >= target {
            return now;
        }
        sprint(server, (target - now).min(3000));
        sync(bot).await;
    }
}

/// `/tick step ticks` on the console; returns the game time once the world stands still again.
fn step(server: &Server, ticks: i64) -> i64 {
    let target = game_time(server) + ticks;
    server.cmd(&format!("tick step {ticks}"));
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let now = game_time(server);
        if now >= target {
            return now;
        }
        assert!(Instant::now() < deadline, "the step never finished");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The demo's packets since `*from`: events (`game_event`), system messages of the demo, acknowledged block actions.
fn watched(bot: &Bot, from: &mut usize) -> Vec<String> {
    let text = bot.log_text();
    let lines: Vec<&str> = text.lines().collect();
    let out = lines[(*from).min(lines.len())..]
        .iter()
        .filter_map(|l| l.split_once("ms ").map(|(_, rest)| rest))
        .filter(|r| {
            (r.starts_with("<- play/game_event") && r.contains("event: 5,"))
                || (r.starts_with("<- play/system_chat") && r.contains("demo."))
                || r.starts_with("<- play/block_changed_ack")
        })
        .map(|r| format!("  {r}"))
        .collect();
    *from = lines.len();
    out
}

/// Waits until the bot has read everything the server sent so far: the reply to a command comes after it. (After a long
/// sprint the bot is still decoding what the sprint sent.)
async fn sync(bot: &Bot) {
    let since = bot.chat_len();
    bot.command("list");
    bot.wait_for(Duration::from_secs(300), |s| {
        s.chat
            .iter()
            .skip(since)
            .any(|l| l.json.contains("commands.list"))
            .then_some(())
    })
    .await
    .expect("the reply to /list");
}

/// Waits for the acknowledgement of the bot's last block action or item use: the server sends it after the reminder.
async fn wait_ack(bot: &Bot) {
    let want = bot.with_state(|s| s.next_sequence);
    bot.wait_for(Duration::from_secs(60), |s| {
        (s.acked_sequence >= want).then_some(())
    })
    .await
    .expect("the acknowledgement");
}

fn report(r: &mut Rec<'_>, bot: &Bot, from: &mut usize, step: &str) {
    r.step(step);
    r.out.extend(watched(bot, from));
}

/// The block below the bot.
fn below(bot: &Bot) -> BlockPos {
    let p = bot.position();
    BlockPos::new(
        p.x.floor() as i32,
        p.y.floor() as i32 - 1,
        p.z.floor() as i32,
    )
}

fn dig(bot: &Bot) {
    bot.start_digging(below(bot), Direction::Up);
    bot.abort_digging();
}

fn use_item(bot: &Bot) {
    bot.use_item(InteractionHand::MainHand);
}

async fn timeline(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    r.console("tick freeze");
    // a demo world is on normal difficulty: the idle bot would starve or be killed during the long sprints
    r.console("difficulty peaceful");
    let client = r.join("Bot").await;
    let bot = &client.bot;
    let mut from = 0;
    // the first game-mode tick is the first player tick after the join; the intro is due after the twentieth
    tokio::time::sleep(Duration::from_millis(3000)).await;
    report(&mut r, bot, &mut from, "intro");

    // Every event is passed once: the world rests (and the player keeps ticking, which would repeat an event whose time
    // the world rests at) only at times without one, two ticks before and after it.
    const EVENTS: &[i64] = &[
        100, 175, 250, 500, 24_500, 48_500, 72_500, 96_500, 118_000, 120_500,
    ];
    for &event in EVENTS {
        let rest = advance_to(s, bot, event - 5).await;
        assert!(rest <= event - 3, "the world is already at {rest}");
        sync(bot).await;
        report(&mut r, bot, &mut from, &format!("rest at {rest}"));
        let before = step(s, event - 1 - rest);
        sync(bot).await;
        report(&mut r, bot, &mut from, &format!("step to {before}"));
        let after = step(s, 2);
        sync(bot).await;
        report(&mut r, bot, &mut from, &format!("step to {after}"));
    }

    // The demo is over. A block break or an item use is answered with the reminder once more than 100 player ticks went
    // by since the last one (the world's rest counts: players tick while it is frozen).
    r.console("give Bot minecraft:stick");
    sync(bot).await;
    watched(bot, &mut from);
    let actions = [
        ("dig right after the end", 0, dig as fn(&Bot)),
        ("dig after 200 ticks", 200, dig),
        ("dig at once again", 0, dig),
        ("dig after 60 ticks", 60, dig),
        ("dig after 60 more ticks", 60, dig),
        ("use at once", 0, use_item),
    ];
    for (label, ticks, act) in actions {
        if ticks > 0 {
            sprint(s, ticks);
        }
        act(bot);
        wait_ack(bot).await;
        report(&mut r, bot, &mut from, label);
    }
    sprint(s, 150);
    use_item(bot);
    wait_ack(bot).await;
    report(&mut r, bot, &mut from, "use after 150 ticks");
    r.out.push(format!(
        "> bot {}",
        bot.disconnect_reason()
            .map_or("connected".to_string(), |reason| format!(
                "disconnected: {reason}"
            ))
    ));
    bot.disconnect().await;
    r.out
}

/// `path = value` for every leaf of `tag`, in file order.
fn flatten(path: &str, tag: &Tag, out: &mut Vec<String>) {
    match tag {
        Tag::Compound(c) => {
            for (k, v) in c.iter() {
                flatten(&format!("{path}.{k}"), v, out);
            }
        }
        Tag::List(l) => {
            for (i, v) in l.iter().enumerate() {
                flatten(&format!("{path}[{i}]"), v, out);
            }
        }
        leaf => out.push(format!("{path} = {leaf:?}")),
    }
}

/// Values that depend on when the world was created or saved.
const CLOCKS: &[&str] = &[".Time", ".DayTime", ".LastPlayed"];

async fn level_data(s: &Server) -> Vec<String> {
    let before = log_count(s, "Saved the game");
    s.cmd("save-all flush");
    wait_for_more("the save", before, || log_count(s, "Saved the game"));
    let mut lines = Vec::new();
    flatten(
        "",
        &Tag::Compound(read_dat(&s.dir().join("world/level.dat"))),
        &mut lines,
    );
    lines.retain(|l| {
        !CLOCKS
            .iter()
            .any(|c| l.starts_with(&format!(".Data{c} = ")))
    });
    lines
}

/// The overworld's region files of the world.
fn overworld_regions(s: &Server) -> Vec<std::path::PathBuf> {
    let root = s.dir().join("world");
    list_files(&root)
        .into_iter()
        .filter(|f| f.contains("overworld") || !f.contains("dimensions"))
        .filter(|f| f.contains("region/") && f.ends_with(".mca") && !f.contains("entities"))
        .map(|f| root.join(f))
        .collect()
}

async fn bonus_chest(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    let before = log_count(s, "Saved the game");
    s.cmd("save-all flush");
    wait_for_more("the save", before, || log_count(s, "Saved the game"));
    let mut chests = Vec::new();
    for file in overworld_regions(s) {
        for ((cx, cz), chunk) in read_region(&file) {
            for entity in chunk
                .get_list("block_entities")
                .into_iter()
                .flat_map(|l| l.iter())
            {
                let Tag::Compound(entity) = entity else {
                    continue;
                };
                // the chunks around it that features reached hold chests of their own (a monster room's)
                if entity.get_string_or("id", "") == "minecraft:chest"
                    && entity.get_string_or("LootTable", "") == "minecraft:chests/spawn_bonus_chest"
                {
                    chests.push((cx, cz, entity.clone()));
                }
            }
        }
    }
    r.step(&format!("chests {}", chests.len()));
    let Some((cx, cz, chest)) = chests.into_iter().next() else {
        return r.out;
    };
    let (x, y, z) = (
        chest.get_int_or("x", 0),
        chest.get_int_or("y", 0),
        chest.get_int_or("z", 0),
    );
    r.out.push(format!("  chunk {cx} {cz}"));
    for (k, v) in chest.iter() {
        if k == "x" || k == "y" || k == "z" || k == "LootTableSeed" {
            continue;
        }
        r.out.push(format!("  {k} = {v:?}"));
    }

    let client = r.join("Bot").await;
    let bot = &client.bot;
    s.cmd(&format!("tp Bot {x}.5 {} {z}.5", y + 1));
    tokio::time::sleep(Duration::from_secs(3)).await;
    let seed_lines = |s: &Server| log_count(s, "has the following block data");
    s.cmd(&format!(
        "data merge block {x} {y} {z} {{LootTableSeed:42L}}"
    ));
    let before = seed_lines(s);
    s.cmd(&format!("data get block {x} {y} {z} LootTableSeed"));
    wait_for_more("the loot seed", before, || seed_lines(s));
    let log = s.log();
    let seed = log
        .lines()
        .rev()
        .find_map(|l| l.split_once("has the following block data: "))
        .map_or("<none>", |(_, v)| v);
    r.step(&format!("loot seed {seed}"));
    // where the chest stands depends on the level's random: only what holds for every position is compared
    let chest_pos = BlockPos::new(x, y, z);
    r.step("surroundings");
    let state = |pos| bot.block_at(pos).map(|b| format!("{b:?}"));
    let solid = |pos| state(pos).is_some_and(|b| !b.contains("minecraft:air"));
    r.out.push(format!(
        "  chest on a block {}",
        solid(chest_pos.relative(Direction::Down))
    ));
    let torches = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ]
    .map(|d| chest_pos.relative(d))
    .into_iter()
    .filter(|p| state(*p).is_some_and(|b| b.contains("minecraft:torch")))
    .collect::<Vec<_>>();
    r.out.push(format!(
        "  every torch on a block {}",
        torches.iter().all(|p| solid(p.relative(Direction::Down)))
    ));
    r.step("open");
    bot.use_item_on(
        InteractionHand::MainHand,
        chest_pos,
        Direction::Up,
        (0.5, 1.0, 0.5),
        false,
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    let menu = bot.with_state(|st| st.open_menu.clone());
    match menu {
        None => r.out.push("  no menu".into()),
        Some(menu) => {
            r.out
                .push(format!("  {:?} {:?}", menu.menu_type, menu.title));
            for (i, stack) in menu.slots.iter().take(27).enumerate() {
                if !stack.is_empty() {
                    r.out
                        .push(format!("  slot {i} {:?} x{}", stack.item(), stack.count()));
                }
            }
        }
    }
    bot.disconnect().await;
    r.out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn demo_dual_vanilla() {
    record_vanilla(DIR, ONLY, CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn demo_dual_ours() {
    compare_ours(DIR, ONLY, CASES).await;
}
