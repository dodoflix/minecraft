//! Nether portal round trip through the vanilla server and OUR server with the same bot script (see `common/dual.rs`):
//! build an obsidian frame in the flat overworld, light it with `fire` (BaseFireBlock.onPlace), stand in it, arrive in the
//! Nether (PortalForcer.createPortal on the real nether terrain), step out, step back in and return to the overworld portal.
//!
//! `portal_dual_vanilla` checks the vanilla observations against `tests/fixtures/portal_dual/*.txt` (`PORTAL_DUAL_UPDATE=1`
//! rewrites them); `portal_dual_ours` diffs our server's observations against them (`run/portal-dual-ours.diff`,
//! `PORTAL_DUAL_STRICT=1` makes differences a failure).
mod common;
use std::sync::Arc;
use std::time::Duration;

use common::dual::*;
use common::*;
use mc_core::pos::block_pos::BlockPos;
use mc_data::Block;
use mc_testbot::harness::{OurServer, VanillaServer, repo_root, vanilla_available};

impl Console for OurServer {
    fn cmd(&self, command: &str) {
        OurServer::cmd(self, command)
    }
    fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        OurServer::wait_log(self, regex, timeout_s)
    }
}

const PORTAL_PROPS: &[&str] = &[
    "level-type=minecraft:flat",
    "difficulty=normal",
    "spawn-monsters=false",
    "spawn-animals=false",
    "spawn-npcs=false",
    "gamemode=survival",
    "force-gamemode=true",
    "generate-structures=false",
];

fn fixture_path(scenario: &str) -> std::path::PathBuf {
    repo_root().join("crates/mc-testbot/tests/fixtures/portal_dual").join(format!("{scenario}.txt"))
}

const NETHER: &str = "minecraft:the_nether";
const OVERWORLD: &str = "minecraft:overworld";

fn dimension(c: &Ctx) -> String {
    c.bot.with_state(|s| s.dimension.to_string())
}

/// Number of portal blocks / obsidian blocks and the minimal portal corner in the loaded client chunks around `center`.
fn scan(c: &Ctx, center: (i32, i32), y_range: (i32, i32)) -> (usize, usize, Option<BlockPos>) {
    c.bot.with_state(|s| {
        let (mut portals, mut obsidian, mut min) = (0, 0, None::<BlockPos>);
        for x in center.0 - 24..=center.0 + 24 {
            for z in center.1 - 24..=center.1 + 24 {
                for y in y_range.0..=y_range.1 {
                    let Some(st) = s.world.block_state(x, y, z) else { continue };
                    if st.block() == Block::NETHER_PORTAL {
                        portals += 1;
                        let p = BlockPos::new(x, y, z);
                        min = Some(match min {
                            None => p,
                            Some(m) if (p.y(), p.x(), p.z()) < (m.y(), m.x(), m.z()) => p,
                            Some(m) => m,
                        });
                    } else if st.block() == Block::OBSIDIAN {
                        obsidian += 1;
                    }
                }
            }
        }
        (portals, obsidian, min)
    })
}

fn corner(p: Option<BlockPos>) -> String {
    p.map_or("none".into(), |p| format!("{},{},{}", p.x(), p.y(), p.z()))
}

/// Frame, ignition, arrival in the Nether (platform / portal location), step out, return.
async fn portal_roundtrip(c: &Ctx) -> Vec<String> {
    let mut out = Vec::new();
    c.cmd("gamerule players_nether_portal_default_delay 20").await;
    c.reset().await;
    // frame along X at z=2: interior x=4..5, y=-60..-58 on grass (y=-61 becomes the obsidian base)
    for cmd in [
        "fill 4 -61 2 5 -61 2 minecraft:obsidian",
        "fill 4 -57 2 5 -57 2 minecraft:obsidian",
        "fill 3 -60 2 3 -58 2 minecraft:obsidian",
        "fill 6 -60 2 6 -58 2 minecraft:obsidian",
        "setblock 4 -60 2 minecraft:fire",
    ] {
        c.cmd(cmd).await;
    }
    c.ticks(10).await;
    let (portals, obsidian, min) = scan(c, (5, 2), (-64, -50));
    out.push(Obs::new("lit").exact("portal_blocks", portals).exact("obsidian", obsidian).exact("min", corner(min)).line());

    // step in: the transition happens after the (shortened) portal delay
    c.srv.cmd("tp Bot 4.5 -60 2.5");
    let entered = c.bot.wait_for(Duration::from_secs(30), |s| (s.dimension.to_string() == NETHER).then_some(())).await;
    c.ticks(40).await;
    let pos = c.bot.position();
    let (yaw, pitch) = c.bot.with_state(|s| (s.body.yaw, s.body.pitch));
    let (np, no, nmin) = scan(c, (pos.x.floor() as i32, pos.z.floor() as i32), (0, 255));
    out.push(
        Obs::new("arrive")
            .exact("entered", entered.is_ok())
            .exact("dimension", dimension(c))
            .abs("x", pos.x, 0.3)
            .abs("y", pos.y, 0.3)
            .abs("z", pos.z, 0.3)
            .exact("yaw", yaw)
            .exact("pitch", pitch)
            .exact("portal_blocks", np)
            .exact("obsidian", no)
            .exact("min", corner(nmin))
            .line(),
    );

    // leave the portal (3 blocks along z, outside the portal plane) and come back in: return trip
    c.srv.cmd(&format!("execute in minecraft:the_nether run tp Bot {} {} {}", pos.x, pos.y, pos.z + 3.0));
    c.ticks(40).await;
    c.srv.cmd(&format!("execute in minecraft:the_nether run tp Bot {} {} {}", pos.x, pos.y, pos.z));
    let returned = c.bot.wait_for(Duration::from_secs(30), |s| (s.dimension.to_string() == OVERWORLD).then_some(())).await;
    c.ticks(40).await;
    if std::env::var("PORTAL_DUAL_DEBUG").is_ok() {
        for l in c.bot.log_text().lines().filter(|l| l.contains("respawn") || l.contains("player_position") || l.contains("accept_teleport") || l.contains("login")) {
            eprintln!("{l}");
        }
    }
    let back = c.bot.position();
    let (portals, obsidian, min) = scan(c, (5, 2), (-64, -50));
    out.push(
        Obs::new("return")
            .exact("returned", returned.is_ok())
            .exact("dimension", dimension(c))
            .abs("x", back.x, 0.3)
            .abs("y", back.y, 0.3)
            .abs("z", back.z, 0.3)
            .exact("portal_blocks", portals)
            .exact("obsidian", obsidian)
            .exact("min", corner(min))
            .line(),
    );
    out
}

async fn run(bot: &Arc<mc_testbot::Bot>, srv: Arc<dyn Console + Send + Sync>) -> Vec<(String, Vec<String>)> {
    let ctx = Arc::new(Ctx { bot: bot.clone(), srv });
    let lines = match tokio::time::timeout(Duration::from_secs(300), tokio::spawn({
        let ctx = ctx.clone();
        async move { portal_roundtrip(&ctx).await }
    }))
    .await
    {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => vec![format!("portal_roundtrip ERROR={}", e.to_string().replace(' ', "_"))],
        Err(_) => vec!["portal_roundtrip ERROR=timeout".to_string()],
    };
    println!("SCENARIO portal_roundtrip:\n  {}", lines.join("\n  "));
    vec![("portal_roundtrip".to_string(), lines)]
}

#[tokio::test(flavor = "multi_thread")]
async fn portal_dual_vanilla() {
    if !vanilla_available() {
        eprintln!("skipping: vanilla oracle unavailable");
        return;
    }
    let server = Arc::new(VanillaServer::start("tb-portal", 12345, "survival", PORTAL_PROPS).expect("start vanilla"));
    let bot = join("127.0.0.1", server.port, "Bot").await;
    setup(&bot, &*server).await;
    let results = run(&bot, server.clone()).await;
    bot.disconnect().await;
    let update = std::env::var("PORTAL_DUAL_UPDATE").is_ok();
    let mut failures = Vec::new();
    for (name, lines) in &results {
        let path = fixture_path(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, lines.join("\n") + "\n").unwrap();
            continue;
        }
        let expected: Vec<String> = std::fs::read_to_string(&path).unwrap_or_default().lines().map(str::to_string).collect();
        let diffs = compare(&expected, lines);
        if !diffs.is_empty() {
            failures.push(format!("{name}:\n{}", diffs.join("\n")));
        }
    }
    assert!(failures.is_empty(), "vanilla observations differ from the committed fixtures:\n{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn portal_dual_ours() {
    let Some(bin) = OurServer::binary() else {
        eprintln!("skipping: no mc-server binary");
        return;
    };
    let server = Arc::new(OurServer::start_with_props(&bin, 12345, "survival", PORTAL_PROPS).expect("start our server"));
    let bot = join("127.0.0.1", server.port, "Bot").await;
    setup(&bot, &*server).await;
    let results = run(&bot, server.clone()).await;
    bot.disconnect().await;
    let mut report = String::new();
    for (name, lines) in &results {
        let expected: Vec<String> = std::fs::read_to_string(fixture_path(name)).unwrap_or_default().lines().map(str::to_string).collect();
        let diffs = compare(&expected, lines);
        if diffs.is_empty() {
            report.push_str(&format!("MATCH {name}\n"));
        } else {
            report.push_str(&format!("DIFF  {name}\n{}\n", diffs.join("\n")));
        }
    }
    let _ = std::fs::create_dir_all(repo_root().join("run"));
    let _ = std::fs::write(repo_root().join("run/portal-dual-ours.diff"), &report);
    println!("{report}");
    if std::env::var("PORTAL_DUAL_STRICT").is_ok() {
        assert!(!report.contains("DIFF "), "our server differs from vanilla:\n{report}");
    }
}
