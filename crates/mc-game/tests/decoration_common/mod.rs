//! Shared by the decoration goldens: the fixture, the generators it names and the placed feature names.
#![allow(dead_code)]

use flate2::read::GzDecoder;
use mc_core::ident::Identifier;
use mc_worldgen::biome::{BiomeRegistry, PlacedFeatureRef};
use mc_worldgen::data::{McDataSource, WorldgenRegistries};
use mc_worldgen::generator::ChunkGenerator;
use mc_worldgen::world_preset::level_generator_from_json;
use serde_json::json;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

pub fn fixture() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/decoration/decoration.txt.gz");
    let mut text = String::new();
    GzDecoder::new(std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).read_to_string(&mut text).unwrap();
    text
}

/// The placed feature as the oracle names it.
pub fn feature_name(feature: &PlacedFeatureRef) -> String {
    match feature {
        PlacedFeatureRef::Named(id) => id.to_string(),
        PlacedFeatureRef::InlineFillLayer { height, state } => format!("FILL_LAYER:{height}:{}", state.block().data().name),
    }
}

pub struct Generators {
    pub registries: Arc<WorldgenRegistries>,
    pub biomes: Arc<BiomeRegistry>,
    /// `GEN` lines of the fixture: name -> generator.
    pub generators: Vec<(String, Arc<dyn ChunkGenerator>)>,
}

/// Builds every generator the fixture names (`GEN <name> <flat settings | ->`).
pub fn generators(text: &str) -> Generators {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(McDataSource)));
    let biomes = Arc::new(BiomeRegistry::load(&registries).unwrap());
    let preset = registries.raw("worldgen/world_preset", &Identifier::parse("minecraft:normal").unwrap()).unwrap();
    let mut generators = Vec::new();
    for line in text.lines().filter_map(|l| l.strip_prefix("GEN ")) {
        let (name, flat) = line.split_once(' ').unwrap();
        let generator_json = if flat == "-" {
            preset["dimensions"][format!("minecraft:{name}")]["generator"].clone()
        } else {
            json!({"type": "minecraft:flat", "settings": serde_json::from_str::<serde_json::Value>(flat).unwrap()})
        };
        let generator = level_generator_from_json(&generator_json, &registries, &biomes).unwrap_or_else(|e| panic!("{name}: {e}"));
        generators.push((name.to_owned(), Arc::from(generator.into_boxed())));
    }
    assert_eq!(generators.len(), 9);
    Generators { registries, biomes, generators }
}
