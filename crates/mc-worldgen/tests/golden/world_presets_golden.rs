//! World presets (`worldgen/world_preset`) vs vanilla: for every registered preset, every dimension's stem
//! (dimension type, generator kind, biome source, noise settings / flat settings) against
//! `tests/fixtures/flat/world_presets.txt.gz` (generator `tests/oracle/FlatOracle.java`).

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use flate2::read::GzDecoder;
use mc_core::ident::Identifier;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{McDataSource, WorldgenRegistries};
use mc_worldgen::flat::settings::StructureSetHolderSet;
use mc_worldgen::data::LoadError;
use mc_worldgen::densityfunction::codec::LoadResult;
use mc_worldgen::world_preset::{
    DimensionTypes, Holder, LevelGenerator, SpecialWorldProperty, WorldPreset, world_presets,
};
use serde_json::Value;

/// `dimension_type` as raw JSON: the typed `DimensionType` lives in the game crate (its codec is tested there).
struct RawTypes(Arc<WorldgenRegistries>);

impl DimensionTypes for RawTypes {
    type Value = Value;

    fn reference(&self, id: &Identifier) -> LoadResult<Arc<Value>> {
        self.0
            .raw("dimension_type", id)
            .map(Arc::new)
            .ok_or_else(|| LoadError::new(format!("Failed to get element {id}")))
    }

    fn direct(&self, json: &Value) -> LoadResult<Arc<Value>> {
        Ok(Arc::new(json.clone()))
    }
}

fn fixture() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flat/world_presets.txt.gz");
    let mut text = String::new();
    GzDecoder::new(std::fs::File::open(path).unwrap()).read_to_string(&mut text).unwrap();
    text
}

fn rle(values: &[String]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < values.len() {
        let mut j = i;
        while j < values.len() && values[j] == values[i] {
            j += 1;
        }
        out.push_str(&format!("{}:{} ", j - i, values[i]));
        i = j;
    }
    out.trim().to_string()
}

#[test]
fn world_presets_match_vanilla() {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(McDataSource)));
    let biomes = Arc::new(BiomeRegistry::load(&registries).unwrap());
    let types = RawTypes(registries.clone());
    let text = fixture();
    let mut checked = 0;
    let mut presets = std::collections::BTreeSet::new();
    for line in text.lines() {
        // DIM <preset> <dimension> type=.. generator=.. biome_source=<json> (settings=.. | flat=..)?
        let Some(rest) = line.strip_prefix("DIM ") else { continue };
        let mut parts = rest.splitn(3, ' ');
        let (preset, dim, tail) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let preset_id = Identifier::parse(preset).unwrap();
        let dim_id = Identifier::parse(dim).unwrap();
        presets.insert(preset.to_string());
        let loaded = world_presets::load(&registries, &biomes, &types, &preset_id).unwrap();
        let stem = loaded.create_world_dimensions();
        let stem = stem.get(&dim_id).unwrap_or_else(|| panic!("{preset} {dim} missing")).clone();

        let type_part = tail.split(" generator=").next().unwrap().strip_prefix("type=").unwrap();
        match &stem.dimension_type {
            Holder::Reference(id, _) => assert_eq!(id.to_string(), type_part, "{preset} {dim}"),
            Holder::Direct(_) => assert_eq!("<inline>", type_part),
        }
        let generator_name = tail.split(" generator=").nth(1).unwrap().split(' ').next().unwrap();
        assert_eq!(stem.generator.type_name().to_string(), generator_name, "{preset} {dim}");

        let source_json: Value = {
            let s = tail.split(" biome_source=").nth(1).unwrap();
            let end = s.find("} ").map_or(s.len(), |i| i + 1);
            serde_json::from_str(&s[..end]).unwrap()
        };
        let chunk_generator = stem.generator.as_chunk_generator();
        let possible = chunk_generator.biome_source().possible_biomes();
        match source_json["type"].as_str().unwrap() {
            "minecraft:fixed" => {
                assert_eq!(possible.len(), 1, "{preset} {dim}");
                assert_eq!(biomes.key(possible[0]).to_string(), source_json["biome"].as_str().unwrap());
            }
            ty => match &stem.generator {
                LevelGenerator::Noise(n) => {
                    assert_eq!(n.biome_source_type.to_string(), ty, "{preset} {dim}");
                    // The data JSON of the preset holds the very same biome source object.
                    let data = registries.raw("worldgen/world_preset", &preset_id).unwrap();
                    let json = &data["dimensions"][dim]["generator"]["biome_source"];
                    assert_eq!(json, &source_json, "{preset} {dim}");
                }
                _ => panic!("{preset} {dim}: non-fixed biome source on non-noise generator"),
            },
        }
        match &stem.generator {
            LevelGenerator::Noise(n) => {
                let want = tail.split(" settings=").nth(1).unwrap();
                assert_eq!(n.settings.as_ref().map_or("<inline>".to_string(), |s| s.to_string()), want);
            }
            LevelGenerator::Flat(flat) => {
                let s = flat.settings();
                let mut layers = String::new();
                for l in s.get_layers_info() {
                    layers.push_str(&format!("{}*{},", l.get_height(), l.get_block_state().id()));
                }
                let states: Vec<String> =
                    s.get_layers().iter().map(|s| s.map_or("null".to_string(), |s| s.id().to_string())).collect();
                let so = match s.structure_overrides() {
                    None => "none".to_string(),
                    Some(StructureSetHolderSet::Tag(t)) => format!("tag:{t}"),
                    Some(StructureSetHolderSet::Direct(ids)) => {
                        format!("list:{}", ids.iter().map(|i| format!("{i},")).collect::<String>())
                    }
                };
                let got = format!(
                    "layers={layers} state_layers={} biome={} lakes={} features={} void={} so={so}",
                    rle(&states),
                    biomes.key(s.get_biome()),
                    s.has_lakes(),
                    s.has_decoration(),
                    s.is_void_gen(),
                );
                assert_eq!(format!("flat={got}"), tail.split(" flat=").nth(1).map(|t| format!("flat={t}")).unwrap());
            }
            LevelGenerator::Debug(_) => {}
        }
        checked += 1;
    }
    assert_eq!(presets.len(), 7, "{presets:?}");
    assert_eq!(checked, 21);

    // BOOTSTRAP_DIM lines (vanilla's own bootstrap) must equal the DIM lines (the data pack).
    let dim: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("DIM ")).collect();
    let boot: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("BOOTSTRAP_DIM ")).collect();
    assert_eq!(dim, boot);
}

#[test]
fn preset_helpers() {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(McDataSource)));
    let biomes = Arc::new(BiomeRegistry::load(&registries).unwrap());
    let types = RawTypes(registries.clone());
    let normal = world_presets::create_normal_world_dimensions(&registries, &biomes, &types).unwrap();
    assert_eq!(normal.special_world_property(), SpecialWorldProperty::None);
    assert!(!normal.is_debug());
    let keys: Vec<String> = normal.dimensions().iter().map(|(k, _)| k.to_string()).collect();
    assert_eq!(keys, ["minecraft:overworld", "minecraft:the_nether", "minecraft:the_end"]);
    assert_eq!(world_presets::from_settings(&normal).unwrap().to_string(), "minecraft:normal");
    let debug = world_presets::load(&registries, &biomes, &types, &Identifier::parse("minecraft:debug_all_block_states").unwrap())
        .unwrap()
        .create_world_dimensions();
    assert!(debug.is_debug());
    assert_eq!(debug.special_world_property(), SpecialWorldProperty::Debug);
    let flat = world_presets::load(&registries, &biomes, &types, &Identifier::parse("minecraft:flat").unwrap())
        .unwrap()
        .create_world_dimensions();
    assert_eq!(flat.special_world_property(), SpecialWorldProperty::Flat);
    let amplified: Arc<WorldPreset<Value>> =
        world_presets::load(&registries, &biomes, &types, &Identifier::parse("minecraft:amplified").unwrap()).unwrap();
    assert!(amplified.overworld().is_some());
}
