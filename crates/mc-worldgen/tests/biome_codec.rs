//! Codec/loader behavior of the biome types (JSON shapes accepted/rejected like vanilla's codecs).

use std::sync::Arc;

use serde_json::json;

use mc_core::ident::Identifier;
use mc_worldgen::biome::climate::{Parameter, TargetPoint};
use mc_worldgen::biome::{
    BiomeRegistry, MultiNoiseBiomeSource, Preset, TemperatureModifier, biome_source_from_json,
};
use mc_worldgen::data::{McDataSource, WorldgenRegistries};

fn registries() -> WorldgenRegistries {
    WorldgenRegistries::new(Arc::new(McDataSource))
}

#[test]
fn parameter_codec_accepts_number_list_and_object() {
    assert_eq!(Parameter::from_json(&json!(0.5)).unwrap(), Parameter::new(5000, 5000));
    assert_eq!(Parameter::from_json(&json!([-1.0, 0.25])).unwrap(), Parameter::new(-10000, 2500));
    assert_eq!(
        Parameter::from_json(&json!({"min": -0.45, "max": 1.0})).unwrap(),
        Parameter::new(-4500, 10000)
    );
    assert!(Parameter::from_json(&json!([0.5, 0.1])).is_err(), "min > max");
    assert!(Parameter::from_json(&json!([0.5])).is_err(), "not 2 elements");
    assert!(Parameter::from_json(&json!(2.5)).is_err(), "outside [-2, 2]");
    assert!(Parameter::from_json(&json!("x")).is_err());
}

#[test]
fn registry_resolves_ids_tags_and_lists() {
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).unwrap();
    let plains = biomes.id_of(&Identifier::parse("minecraft:plains").unwrap()).unwrap();
    assert_eq!(biomes.key(plains).to_string(), "minecraft:plains");
    assert_eq!(biomes.holder_set_from_json(&json!("minecraft:plains")).unwrap(), vec![plains]);
    assert_eq!(
        biomes.holder_set_from_json(&json!(["minecraft:plains", "minecraft:plains"])).unwrap(),
        vec![plains, plains]
    );
    let ocean = biomes.holder_set_from_json(&json!("#minecraft:is_ocean")).unwrap();
    let names: Vec<String> = ocean.iter().map(|b| biomes.key(*b).to_string()).collect();
    let tag = mc_data::tags::names("worldgen/biome", "minecraft:is_ocean").unwrap();
    assert_eq!(names, tag);
    assert!(biomes.holder_set_from_json(&json!("minecraft:nope")).is_err());
    assert!(biomes.holder_set_from_json(&json!("#minecraft:nope")).is_err());
}

#[test]
fn biome_climate_settings_are_decoded() {
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).unwrap();
    let frozen = biomes.id_of_name("frozen_ocean").unwrap();
    assert_eq!(biomes.get(frozen).climate().temperature_modifier, TemperatureModifier::Frozen);
    let nether = biomes.id_of_name("nether_wastes").unwrap();
    assert_eq!(biomes.get(nether).generation_settings().carvers.len(), 1);
    assert!(!biomes.get(nether).has_precipitation());
    let plains = biomes.id_of_name("plains").unwrap();
    assert_eq!(biomes.get(plains).generation_settings().carvers.len(), 3);
    assert_eq!(biomes.get(plains).get_base_temperature(), 0.8);
}

#[test]
fn biome_source_codec_dispatches_and_rejects() {
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).unwrap();
    for bad in [
        json!({"type": "minecraft:unknown"}),
        json!({"type": "minecraft:fixed"}),
        json!({"type": "minecraft:fixed", "biome": "minecraft:nope"}),
        json!({"type": "minecraft:multi_noise", "preset": "minecraft:end"}),
        json!({"type": "minecraft:multi_noise", "preset": {"preset": "minecraft:nope"}}),
        json!({"type": "minecraft:checkerboard", "biomes": "minecraft:plains", "scale": 63}),
        json!({"biomes": []}),
    ] {
        assert!(biome_source_from_json(&bad, &registries, &biomes).is_err(), "{bad}");
    }
    // inline holder and registry reference give the same list
    let by_ref = MultiNoiseBiomeSource::from_json(&json!({"preset": "minecraft:nether"}), &registries, &biomes).unwrap();
    let inline = MultiNoiseBiomeSource::from_json(
        &json!({"preset": {"preset": "minecraft:nether"}}),
        &registries,
        &biomes,
    )
    .unwrap();
    assert!(by_ref.stable(&Identifier::parse("minecraft:nether").unwrap()));
    assert!(!inline.stable(&Identifier::parse("minecraft:nether").unwrap()));
    assert_eq!(by_ref.parameter_list().values().len(), Preset::Nether.entries().len());
}

#[test]
fn direct_multi_noise_list_selects_closest_entry() {
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).unwrap();
    let point = |t: f64| {
        json!({"temperature": t, "humidity": 0.0, "continentalness": 0.0, "erosion": 0.0,
               "depth": 0.0, "weirdness": 0.0, "offset": 0.0})
    };
    let source = MultiNoiseBiomeSource::from_json(
        &json!({"biomes": [
            {"biome": "minecraft:desert", "parameters": point(0.5)},
            {"biome": "minecraft:taiga", "parameters": point(-0.5)},
        ]}),
        &registries,
        &biomes,
    )
    .unwrap();
    let target = |t: i64| TargetPoint { temperature: t, humidity: 0, continentalness: 0, erosion: 0, depth: 0, weirdness: 0 };
    assert_eq!(source.get_noise_biome(&target(4000)), biomes.id_of_name("desert").unwrap());
    assert_eq!(source.get_noise_biome(&target(-100)), biomes.id_of_name("taiga").unwrap());
}
