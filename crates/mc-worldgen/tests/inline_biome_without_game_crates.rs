//! This test binary links no game crate, so nothing registers the decoders of an inline biome's `attributes` and inline
//! placed features: such a biome must be refused instead of keeping what could not be checked.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{McDataSource, WorldgenRegistries};

fn plains(registries: &WorldgenRegistries) -> serde_json::Value {
    let id = Identifier::with_default_namespace("plains").unwrap();
    registries.raw("worldgen/biome", &id).unwrap()
}

#[test]
fn inline_biome_attributes_are_refused_without_a_decoder() {
    let registries = WorldgenRegistries::new(Arc::new(McDataSource));
    let biomes = BiomeRegistry::load(&registries).unwrap();
    let mut biome = plains(&registries);
    assert!(biome["attributes"].is_object(), "plains has attributes");
    assert!(biomes.holder_from_json(&biome).is_err());
    biome.as_object_mut().unwrap().remove("attributes");
    assert!(biomes.holder_from_json(&biome).is_ok());
}

#[test]
fn inline_placed_features_are_dropped_without_a_decoder() {
    let registries = WorldgenRegistries::new(Arc::new(McDataSource));
    let biomes = BiomeRegistry::load(&registries).unwrap();
    let mut biome = plains(&registries);
    biome.as_object_mut().unwrap().remove("attributes");
    let steps = biome["features"].as_array_mut().unwrap();
    let step = steps.iter_mut().find(|s| !s.as_array().unwrap().is_empty());
    let step = step.unwrap().as_array_mut().unwrap();
    let before = step.len();
    step.insert(
        0,
        serde_json::json!({"feature": "minecraft:lake_lava", "placement": []}),
    );
    let id = biomes.holder_from_json(&biome).unwrap();
    let kept: usize = biomes
        .get(id)
        .generation_settings()
        .features()
        .iter()
        .map(Vec::len)
        .sum();
    let original: usize = biome["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_array().unwrap().len())
        .sum();
    assert_eq!(kept, original - 1, "only the inline feature is dropped");
    assert!(before > 0);
}
