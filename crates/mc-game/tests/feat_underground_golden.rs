//! Golden test of the underground / End features (geode, speleothem, speleothem_cluster, large_dripstone, monster_room,
//! spike, end_spike, end_gateway, end_island, end_podium, end_platform, fossil, template) against the real vanilla classes: every
//! `worldgen/feature` data-pack entry of those types plus hand-written variants is placed at many origins/seeds over the
//! synthetic terrain (random caverns, solid blocks, snow patches) and the dump of all chunks (blocks, block entity NBT,
//! heightmaps, post-processing marks, scheduled ticks, entities, region random) plus the feature RNG state after each placement
//! is compared line by line. Fixture `tests/fixtures/feat_underground.txt.gz` (gzip of the output of `tests/oracle/UndergroundOracle.java`).

mod common;

use mc_game::world::level::levelgen::structure::templatesystem::structure_processor_list::load_processor_lists;
use std::io::Read;

use common::feature_harness::*;
use mc_core::pos::BlockPos;
use mc_core::random::{RandomSource, XoroshiroRandomSource};
use mc_game::world::level::levelgen::GenEnv;
use mc_game::world::level::levelgen::WorldGenLevel;
use mc_game::world::level::levelgen::feature::{Feature, decode_direct};
use mc_game::world::level::levelgen::registries::{Cells, DecodeCtx};
use mc_game::world::level::LevelAccessor;
use mc_world::status::ChunkStatus;
use mc_worldgen::data::{FsDataSource, WorldgenDataSource};
use std::sync::Arc;

fn int(s: &str) -> i32 {
    s.parse().unwrap_or_else(|_| panic!("int {s}"))
}

/// Entity UUIDs are random in vanilla; the oracle strips them from its dump.
fn strip_uuid(line: &str) -> String {
    match line.find("UUID:[I;") {
        Some(start) => {
            let end = line[start..].find(']').map_or(line.len(), |e| start + e + 1);
            format!("{}UUID:[I;0]{}", &line[..start], &line[end..])
        }
        None => line.to_string(),
    }
}

fn decode(json: &str) -> Result<Arc<dyn Feature>, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(root));
    let cells = Cells::new(source);
    let ctx = DecodeCtx { worldgen: &WORLD.worldgen, biomes: &WORLD.biomes, cells: &cells };
    // the `worldgen/processor_list` holders referenced by id (fossil, template)
    load_processor_lists(&ctx).expect("processor lists decode");
    let value: serde_json::Value = serde_json::from_str(json).expect("fixture json");
    decode_direct(&value, &ctx).map_err(|e| e.to_string())
}

fn exec(
    level: &mut mc_game::world::level::levelgen::gen_level::GenLevel<'_>,
    env: &Arc<GenEnv>,
    feature: &dyn Feature,
    op: &str,
) -> String {
    let t: Vec<&str> = op.split(' ').collect();
    if t[0] == "fill" {
        let state = parse_state(t[7]);
        let (x1, y1, z1, x2, y2, z2) = (int(t[1]), int(t[2]), int(t[3]), int(t[4]), int(t[5]), int(t[6]));
        for y in y1.min(y2)..=y1.max(y2) {
            for z in z1.min(z2)..=z1.max(z2) {
                for x in x1.min(x2)..=x1.max(x2) {
                    level.set_block(BlockPos::new(x, y, z), state, 2, 512);
                }
            }
        }
        return "ok".into();
    }
    assert_eq!(t[0], "place", "{op}");
    let mut random = XoroshiroRandomSource::new(t[4].parse().expect("seed"));
    let placed = feature.place(level, env, &mut random, BlockPos::new(int(t[1]), int(t[2]), int(t[3])));
    format!("{placed} {}", random.next_long())
}

#[test]
fn underground_features_match_vanilla() {
    run(false);
}

/// `fossil` / `template` place structure templates: `StructureTemplate.placeInWorld` updates the shapes of the placed blocks
/// (`RotatedPillarBlock.rotate`, `FallingBlock`/`LiquidBlock` tick scheduling, `SulfurSpike` thickness, ...), i.e. needs the
/// concrete block classes (WorldgenBlocks, not on main yet).
#[test]
#[ignore = "needs the concrete block classes (WorldgenBlocks): rotate / updateShape / onPlace of the placed template blocks"]
fn template_features_match_vanilla() {
    run(true);
}

fn run(templates: bool) {
    let mut fixture = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/feat_underground.txt.gz")).expect("fixture present"))
        .read_to_string(&mut fixture)
        .expect("fixture is gzip");
    let mut lines = fixture.lines();
    let mut feature: Option<Arc<dyn Feature>> = None;
    let (mut cases, mut scenarios, mut places, mut dump_lines, mut rejected) = (0, 0, 0, 0, 0);
    let mut placed_true = 0;
    let mut failures: Vec<String> = Vec::new();
    while let Some(line) = lines.next() {
        let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
        match head {
            "BAD" => {
                assert_eq!(lines.next(), Some("= ERR"));
                match decode(rest) {
                    Ok(_) => failures.push(format!("codec accepted what vanilla rejects: {rest}")),
                    Err(_) => rejected += 1,
                }
            }
            "CASE" => {
                let mut it = rest.splitn(3, ' ');
                let (id, kind, json) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
                match decode(json) {
                    Ok(_) if matches!(kind, "fossil" | "template") != templates => feature = None,
                    Ok(f) => feature = Some(f),
                    Err(e) => {
                        failures.push(format!("case {id} does not decode: {e}"));
                        feature = None;
                    }
                }
                cases += usize::from(feature.is_some());
            }
            "SCENARIO" => {
                let h: Vec<&str> = rest.split(' ').collect();
                let (id, n, seed, cx, cz) = (h[0], h[1], h[2].parse::<i64>().unwrap(), int(h[3]), int(h[4]));
                let mut script: Vec<(&str, &str)> = Vec::new();
                let mut expected = Vec::new();
                let mut snap_at = None;
                while let Some(line) = lines.next() {
                    if line == "END" {
                        break;
                    }
                    if line == "SNAP" {
                        snap_at = Some(script.len());
                    } else if let Some(op) = line.strip_prefix("> ") {
                        script.push((op, lines.next().and_then(|l| l.strip_prefix("= ")).expect("result line")));
                    } else if let Some(d) = line.strip_prefix("D ") {
                        expected.push(d.to_string());
                    } else {
                        panic!("bad fixture line {line}");
                    }
                }
                let Some(f) = feature.as_deref() else { continue };
                let snap_at = snap_at.expect("SNAP line");
                let mut synth = SynthRegion::new(seed, cx, cz, ChunkStatus::Features, 1);
                let env = synth.env.clone();
                // terrain preparation first: the dump lists the blocks changed since the snapshot taken after it
                {
                    let mut level = synth.level();
                    for (op, _) in &script[..snap_at] {
                        exec(&mut level, &env, f, op);
                    }
                }
                let before = snapshot(&synth);
                let mut level = synth.level();
                for (op, want) in &script[snap_at..] {
                    let got = exec(&mut level, &env, f, op);
                    places += 1;
                    placed_true += usize::from(got.starts_with("true"));
                    if got != *want && failures.len() < 30 {
                        failures.push(format!("{id}/{n} op `{op}`: got `{got}`, vanilla `{want}`"));
                    }
                }
                let got: Vec<String> = dump(&mut level, &before).iter().map(|l| strip_uuid(l)).collect();
                dump_lines += expected.len();
                if got != expected && failures.len() < 30 {
                    let first = got.iter().zip(&expected).position(|(a, b)| a != b).unwrap_or(got.len().min(expected.len()));
                    failures.push(format!(
                        "{id}/{n} dump differs at line {first}: ours `{}`, vanilla `{}` ({} vs {} lines)",
                        got.get(first).map_or("<none>", String::as_str),
                        expected.get(first).map_or("<none>", String::as_str),
                        got.len(),
                        expected.len()
                    ));
                }
                scenarios += 1;
            }
            other => panic!("bad fixture line {other}"),
        }
    }
    assert!(failures.is_empty(), "{cases} cases, {scenarios} scenarios, {places} placements ({placed_true} placed), {rejected} rejected, {dump_lines} dump lines:\n{}", failures.join("\n"));
    if templates {
        assert!(cases >= 10 && places > 50 && placed_true > 20, "fixture too small: {cases} cases, {places} placements, {placed_true} placed");
        return;
    }
    assert!(cases >= 20 && places > 100 && placed_true > 40 && rejected >= 40, "fixture too small: {cases} cases, {places} placements, {placed_true} placed, {rejected} rejected");
}
