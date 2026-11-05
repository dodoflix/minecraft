//! Player `.dat` parity: the same scenario runs against the real vanilla server and ours (offline "Bot", survival,
//! same seed), the saved `world/players/data/<uuid>.dat` files are diffed key by key (paths, tag types, values, key
//! order, list order). Needs the `mc-server` binary (`MC_SERVER_BIN`) and the oracle.
mod common;
use std::path::Path;
use std::time::Duration;

use common::*;
use mc_nbt::{CompoundTag, Tag};
use mc_testbot::harness::{OurServer, VanillaServer, vanilla_available};

fn read_dat(dir: &Path) -> CompoundTag {
    let data = dir.join("world/players/data");
    let file = std::fs::read_dir(&data).unwrap_or_else(|e| panic!("{}: {e}", data.display())).flatten().find(|e| e.path().extension().is_some_and(|x| x == "dat")).expect("player file").path();
    let bytes = std::fs::read(file).expect("read dat");
    mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap()).expect("nbt")
}

impl Console for OurServer {
    fn cmd(&self, command: &str) {
        OurServer::cmd(self, command)
    }
    fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        OurServer::wait_log(self, regex, timeout_s)
    }
}

fn kind(t: &Tag) -> &'static str {
    match t {
        Tag::End => "end",
        Tag::Byte(_) => "byte",
        Tag::Short(_) => "short",
        Tag::Int(_) => "int",
        Tag::Long(_) => "long",
        Tag::Float(_) => "float",
        Tag::Double(_) => "double",
        Tag::ByteArray(_) => "byte[]",
        Tag::String(_) => "string",
        Tag::List(_) => "list",
        Tag::Compound(_) => "compound",
        Tag::IntArray(_) => "int[]",
        Tag::LongArray(_) => "long[]",
    }
}

fn diff_tag(path: &str, v: &Tag, o: &Tag, out: &mut Vec<String>) {
    match (v, o) {
        (Tag::Compound(a), Tag::Compound(b)) => diff_compound(path, a, b, out),
        (Tag::List(a), Tag::List(b)) => {
            let (a, b): (Vec<&Tag>, Vec<&Tag>) = (a.iter().collect(), b.iter().collect());
            if a.len() != b.len() {
                out.push(format!("{path}: list length vanilla {} ours {}", a.len(), b.len()));
            }
            for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                diff_tag(&format!("{path}[{i}]"), x, y, out);
            }
        }
        _ if kind(v) != kind(o) => out.push(format!("{path}: type vanilla {} ({v:?}) ours {} ({o:?})", kind(v), kind(o))),
        _ if v != o => out.push(format!("{path}: vanilla {v:?} ours {o:?}")),
        _ => {}
    }
}

fn diff_compound(path: &str, v: &CompoundTag, o: &CompoundTag, out: &mut Vec<String>) {
    for (k, vv) in v.iter() {
        match o.get(k) {
            None => out.push(format!("{path}.{k}: missing in ours (vanilla {vv:?})")),
            Some(ov) => diff_tag(&format!("{path}.{k}"), vv, ov, out),
        }
    }
    for (k, ov) in o.iter() {
        if v.get(k).is_none() {
            out.push(format!("{path}.{k}: extra in ours ({ov:?})"));
        }
    }
    let common = |c: &CompoundTag, other: &CompoundTag| c.keys().filter(|k| other.get(k).is_some()).cloned().collect::<Vec<_>>();
    let (kv, ko) = (common(v, o), common(o, v));
    if kv != ko {
        out.push(format!("{path}: key order vanilla {kv:?} ours {ko:?}"));
    }
}

async fn scenario(console: &(dyn Console + Sync), port: u16, walk: bool) {
    let bot = join("127.0.0.1", port, "Bot").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    if walk {
        let start = bot.position();
        let _ = bot.walk_to(start.x + 6.0, start.z, 0.4, true, Duration::from_secs(12)).await;
        bot.stop();
        tokio::time::sleep(Duration::from_secs(8)).await;
    }
    console.cmd("save-all flush");
    console.wait_log("Saved the game", 60).expect("saved");
    bot.disconnect().await;
}

async fn run_both(walk: bool, name: &str) -> (CompoundTag, CompoundTag) {
    let bin = OurServer::binary().expect("MC_SERVER_BIN");
    let van = VanillaServer::start(&format!("pd-van-{name}"), 12345, "survival", &[]).expect("vanilla");
    scenario(&van, van.port, walk).await;
    van.stop();
    let vanilla = read_dat(&van.dir());
    let mut ours = OurServer::start(&bin, 12345, "survival").expect("ours");
    scenario(&ours, ours.port, walk).await;
    assert!(ours.stop_and_wait(60));
    let o = read_dat(ours.dir());
    (vanilla, o)
}

#[tokio::test(flavor = "multi_thread")]
async fn playerdata_matches_vanilla() {
    if OurServer::binary().is_none() || !vanilla_available() {
        eprintln!("skipping: no mc-server binary or no vanilla oracle");
        return;
    }
    let mut report = Vec::new();
    for (walk, name) in [(false, "new"), (true, "walked")] {
        let (mut v, mut o) = run_both(walk, name).await;
        sort_attributes(&mut v);
        sort_attributes(&mut o);
        let mut d = Vec::new();
        diff_compound("", &v, &o, &mut d);
        println!("=== {name}: {} differences", d.len());
        for l in &d {
            println!("{l}");
        }
        println!("--- vanilla {name}:\n{v:?}\n--- ours {name}:\n{o:?}");
        report.extend(d.into_iter().map(|l| format!("{name}{l}")));
    }
    let unexpected: Vec<_> = report.iter().filter(|l| !EXPECTED.iter().any(|e| l.contains(e))).collect();
    assert!(unexpected.is_empty(), "unexpected player data differences:\n{}", unexpected.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
}

/// Differences that cannot be equal run to run (random per player/world, wall-clock driven); everything else must match.
const EXPECTED: &[&str] = &[
    // random: `ServerPlayer.fudgeSpawnLocation` offsets the spawn inside the spawn radius; walking is wall-clock driven
    ".Pos[",
    // wall-clock/tick-count driven (join -> save-all latency differs between the two servers)
    ".foodSaturationLevel",
    ".foodExhaustionLevel",
    ".warden_spawn_tracker.ticks_since_last_warning",
    // known gaps, remove the entry when fixed: fresh player `Fire` is -20 in vanilla (`applyEffectsFromBlocks`; EntitiesCore2),
    // `recipeBook` needs the recipe manager (vanilla unlocks `crafting_table` on join; ItemsMenusScope.ImRecipes)
    ".Fire:",
    ".recipeBook.",
];

/// `attributes` is written in `Object2ObjectOpenHashMap<Holder<Attribute>, _>` order; `Holder.Reference` hashes by
/// identity, so even two vanilla runs disagree: compare it as a set (sorted by id).
fn sort_attributes(c: &mut CompoundTag) {
    if let Some(list) = c.get_list("attributes") {
        let mut v: Vec<_> = list.iter().cloned().collect();
        v.sort_by_key(|t| t.as_compound().and_then(|c| c.get_string("id")).unwrap_or("").to_string());
        let mut l = mc_nbt::ListTag::new();
        for t in v {
            l.push(t);
        }
        c.put("attributes", l);
    }
}
