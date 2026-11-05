//! Shared driver of the structure golden tests (nether fortress, mineshaft, stronghold): reads a fixture written by an oracle
//! built on `oracle/StructureGoldenBase.java` (format documented there), regenerates every start with the Rust port, compares
//! every piece's saved tag and every `placeInChunk` region dump, and returns coverage statistics for the test's own asserts.
//!
//! Debugging: with the environment variable `NOFOLD` the dump compares every block line (generate the fixture with the same
//! variable set, `FIXTURE=<path>` overrides the fixture file).

use super::feature_harness::*;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::ChunkPos;
use mc_core::random::{LegacyRandomSource, WorldgenRandom};
use mc_game::world::level::levelgen::holder::Holder;
use mc_game::world::level::levelgen::registries::{Cells, DecodeCtx};
use mc_game::world::level::levelgen::structure::piece::StructurePieceSerializationContext;
use mc_game::world::level::levelgen::structure::structure::{GenerationContext, Structure, decode_direct};
use mc_game::world::level::levelgen::structure::{StructureManager, StructureStart};
use mc_nbt::Tag;
use mc_world::chunk::LevelHeight;
use mc_world::palette::BiomeId;
use mc_world::status::ChunkStatus;
use mc_worldgen::data::{FsDataSource, WorldgenDataSource};
use std::collections::BTreeMap;
use std::sync::Arc;

const UNDERGROUND_DECORATION: i32 = 6;

/// `data/minecraft/worldgen/structure/<name>.json` decoded by itself (the full structure registry needs structure types that
/// are not ported yet).
pub fn structure(name: &str) -> Holder<dyn Structure> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join(format!("minecraft/worldgen/structure/{name}.json"))).expect("structure json")).unwrap();
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(root));
    let cells = Cells::new(source);
    let ctx = DecodeCtx { worldgen: &WORLD.worldgen, biomes: &WORLD.biomes, cells: &cells };
    Holder::direct(decode_direct(&json, &ctx).unwrap_or_else(|e| panic!("{name}.json: {e:?}")))
}

/// Mirror of `StructureGoldenBase.fold`: the `B` lines of each chunk as one `B* cx cz count fnv` line.
fn fold(lines: Vec<String>) -> Vec<String> {
    if std::env::var_os("NOFOLD").is_some() {
        return lines;
    }
    let mut out = Vec::new();
    let mut group: Option<(i32, i32, u64, Fnv)> = None;
    let flush = |group: &mut Option<(i32, i32, u64, Fnv)>, out: &mut Vec<String>| {
        if let Some((cx, cz, count, fnv)) = group.take() {
            out.push(format!("B* {cx} {cz} {count} {}", fnv.hex()));
        }
    };
    for line in lines {
        if line.starts_with("B ") {
            let t: Vec<&str> = line.split(' ').collect();
            let (x, z) = (t[1].parse::<i32>().unwrap().div_euclid(16), t[3].parse::<i32>().unwrap().div_euclid(16));
            if group.as_ref().is_some_and(|g| (g.0, g.1) != (x, z)) {
                flush(&mut group, &mut out);
            }
            let g = group.get_or_insert_with(|| (x, z, 0, Fnv::default()));
            g.3.s(&line);
            g.2 += 1;
        } else {
            flush(&mut group, &mut out);
            out.push(line);
        }
    }
    flush(&mut group, &mut out);
    out
}

/// `Structure.generate` of the Rust port for `(seed, chunk)` over the real overworld generator.
pub fn generate(structure: &Holder<dyn Structure>, seed: i64, cx: i32, cz: i32) -> StructureStart {
    let env = gen_env(seed);
    let valid = |_: BiomeId| true;
    let height: LevelHeight = level_height();
    let mut context = GenerationContext::new(&env, seed, ChunkPos::new(cx, cz), height, &valid);
    let stub = structure.value().find_valid_generation_point(&mut context).expect("the structure always has a generation point");
    let builder = stub.get_pieces_builder(&mut context);
    StructureStart::new(structure.clone(), ChunkPos::new(cx, cz), 0, builder.build())
}

/// Coverage of a fixture run: counters plus how often each piece id (`P` lines) and block entity / loot table id (`E` lines)
/// occurs.
#[derive(Default)]
pub struct Stats {
    pub starts: usize,
    pub pieces: usize,
    pub placements: usize,
    pub dump_lines: usize,
    pub piece_ids: BTreeMap<String, usize>,
    pub block_entity_ids: BTreeMap<String, usize>,
    pub loot_tables: BTreeMap<String, usize>,
}

/// The first `key:"value"` string of an SNBT line.
fn snbt_string<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("{key}:\"");
    let from = line.find(&marker)? + marker.len();
    Some(&line[from..from + line[from..].find('"')?])
}

/// Runs the whole fixture; panics listing every difference.
pub fn run_fixture(fixture_name: &str, structure: &Holder<dyn Structure>) -> Stats {
    let path = std::env::var("FIXTURE").unwrap_or_else(|_| format!("{}/tests/fixtures/{fixture_name}", env!("CARGO_MANIFEST_DIR")));
    let fixture = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"));
    let mut lines = fixture.lines().peekable();
    let mut stats = Stats::default();
    let mut failures: Vec<String> = Vec::new();
    while let Some(header) = lines.next() {
        let h: Vec<&str> = header.split(' ').collect();
        assert_eq!(h[0], "START", "{header}");
        let (seed, cx, cz, count) = (h[1].parse::<i64>().unwrap(), h[2].parse::<i32>().unwrap(), h[3].parse::<i32>().unwrap(), h[4].parse::<usize>().unwrap());
        let mut want_pieces = Vec::new();
        for _ in 0..count {
            let line = lines.next().and_then(|l| l.strip_prefix("P ")).expect("piece line");
            *stats.piece_ids.entry(snbt_string(line, "id").unwrap_or("?").to_owned()).or_default() += 1;
            want_pieces.push(line.to_owned());
        }
        let mut start = generate(structure, seed, cx, cz);
        let env = gen_env(seed);
        let context = StructurePieceSerializationContext { env: &env };
        let got_pieces: Vec<String> = start.piece_container.pieces.iter().map(|p| Tag::Compound(p.create_tag(&context)).to_string()).collect();
        stats.pieces += count;
        stats.starts += 1;
        if got_pieces != want_pieces {
            let first = got_pieces.iter().zip(&want_pieces).position(|(a, b)| a != b).unwrap_or(got_pieces.len().min(want_pieces.len()));
            failures.push(format!(
                "start {seed} {cx} {cz}: pieces differ at {first}: ours `{}`, vanilla `{}` ({} vs {} pieces)",
                got_pieces.get(first).map_or("<none>", String::as_str),
                want_pieces.get(first).map_or("<none>", String::as_str),
                got_pieces.len(),
                want_pieces.len()
            ));
            // the placements below depend on the pieces
            while lines.peek().is_some_and(|l| !l.starts_with("START")) {
                lines.next();
            }
            continue;
        }
        while lines.peek().is_some_and(|l| l.starts_with("PLACE")) {
            let place: Vec<&str> = lines.next().unwrap().split(' ').collect();
            let (px, pz) = (place[1].parse::<i32>().unwrap(), place[2].parse::<i32>().unwrap());
            let mut expected = Vec::new();
            for line in lines.by_ref() {
                if line == "END" {
                    break;
                }
                expected.push(line.strip_prefix("D ").expect("dump line").to_owned());
            }
            let mut synth = SynthRegion::new(seed, px, pz, ChunkStatus::Features, 1);
            let before = snapshot(&synth);
            let env = synth.env.clone();
            let mut level = synth.level();
            let mut random = WorldgenRandom::new(Box::new(LegacyRandomSource::new(0)));
            let decoration_seed = random.set_decoration_seed(seed, px * 16, pz * 16);
            random.set_feature_seed(decoration_seed, 0, UNDERGROUND_DECORATION);
            let (min_x, min_z) = (px * 16, pz * 16);
            let writable = BoundingBox::new(min_x, MIN_Y + 1, min_z, min_x + 15, MIN_Y + HEIGHT - 1, min_z + 15);
            let manager = StructureManager::new(&env);
            start.place_in_chunk(&mut level, &manager, &env, &mut random, &writable, ChunkPos::new(px, pz));
            let got = fold(dump(&mut level, &before));
            stats.placements += 1;
            stats.dump_lines += expected.len();
            for line in expected.iter().filter(|l| l.starts_with("E ")) {
                *stats.block_entity_ids.entry(snbt_string(line, "id").unwrap_or("?").to_owned()).or_default() += 1;
                if let Some(table) = snbt_string(line, "LootTable") {
                    *stats.loot_tables.entry(table.to_owned()).or_default() += 1;
                }
            }
            if got != expected {
                let diffs: Vec<String> = got
                    .iter()
                    .zip(&expected)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .take(if std::env::var_os("NOFOLD").is_some() { 8 } else { 1 })
                    .map(|(i, (a, b))| format!("line {i}: ours `{a}`, vanilla `{b}`"))
                    .collect();
                failures.push(format!(
                    "start {seed} {cx} {cz} place {px} {pz}: dump differs ({} vs {} lines) {}",
                    got.len(),
                    expected.len(),
                    diffs.join("; ")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} starts, {} pieces, {} placements, {} failures:\n{}", stats.starts, stats.pieces, stats.placements, failures.len(), failures.join("\n"));
    stats
}
