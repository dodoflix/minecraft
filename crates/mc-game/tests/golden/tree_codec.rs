//! Codecs of the tree types: accept/reject behaviour against the vanilla codecs (`tests/oracle/TreeCodecOracle.java`
//! over `fixtures/tree_codec_cases.in`) and decoding of every tree/fallen_tree entry of the vanilla data pack.

use mc_game::world::level::levelgen::codec::decode_dispatch;
use mc_game::world::level::levelgen::feature::decode_direct;
use mc_game::world::level::levelgen::feature::tree::*;
use mc_game::world::level::levelgen::registries::{Cells, DecodeCtx, load_vanilla_data};
use mc_worldgen::data::{FsDataSource, WorldgenDataSource};
use std::sync::Arc;

fn vanilla_source() -> Arc<dyn WorldgenDataSource> {
    Arc::new(FsDataSource::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data"),
    ))
}

#[test]
fn codec_cases_match_vanilla() {
    let (worldgen, biomes, _) = load_vanilla_data();
    let cells = Cells::new(vanilla_source());
    let ctx = DecodeCtx {
        worldgen: &worldgen,
        biomes: &biomes,
        cells: &cells,
    };
    let golden = include_str!("../fixtures/tree_codec_cases.golden");
    let mut wrong = Vec::new();
    let mut total = 0;
    for line in golden.lines() {
        let mut parts = line.split('\t');
        let (kind, json, expected) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let result = match kind {
            "trunk" => decode_dispatch(&value, &ctx, &TRUNK_PLACER_TYPES).map(|_| ()),
            "foliage" => decode_dispatch(&value, &ctx, &FOLIAGE_PLACER_TYPES).map(|_| ()),
            "size" => decode_dispatch(&value, &ctx, &FEATURE_SIZE_TYPES).map(|_| ()),
            "root" => decode_dispatch(&value, &ctx, &ROOT_PLACER_TYPES).map(|_| ()),
            "decorator" => decode_dispatch(&value, &ctx, &TREE_DECORATOR_TYPES).map(|_| ()),
            "feature" => decode_direct(&value, &ctx).map(|_| ()),
            other => panic!("unknown kind {other}"),
        };
        total += 1;
        let actual = if result.is_ok() { "ok" } else { "err" };
        if actual != expected {
            wrong.push(format!(
                "{kind} {json}\n  vanilla: {expected}, ours: {actual} ({:?})",
                result.err().map(|e| e.to_string())
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {total} cases differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Every `tree` / `fallen_tree` entry of the vanilla data pack decodes (the feature registry as a whole loads once every
/// feature type exists; until then only the tree entries are decoded here).
#[test]
fn vanilla_tree_features_decode() {
    let source = vanilla_source();
    let (worldgen, biomes, _) = load_vanilla_data();
    let cells = Cells::new(source.clone());
    let ctx = DecodeCtx {
        worldgen: &worldgen,
        biomes: &biomes,
        cells: &cells,
    };
    let mut decoded = 0;
    for id in cells.list("worldgen/feature") {
        let json = source.get("worldgen/feature", &id).unwrap();
        let ty = json["type"].as_str().unwrap();
        if ty == "minecraft:tree" || ty == "minecraft:fallen_tree" {
            decode_direct(&json, &ctx).unwrap_or_else(|e| panic!("{id}: {e}"));
            decoded += 1;
        }
    }
    assert_eq!(decoded, 51, "45 tree + 6 fallen_tree entries");
}
