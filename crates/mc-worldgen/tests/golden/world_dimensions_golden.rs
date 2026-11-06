//! `WorldDimensions` codec + `bake` vs vanilla: presets and crafted dimension sets (vanilla-like detection, dimension
//! order, registry lifecycle, special world property, inline dimension types / biomes / noise settings, base
//! registries). Fixture `tests/fixtures/flat/world_dimensions.txt` (generator `tests/oracle/FlatOracle.java`); the
//! fixture is self-describing: every `WD_INPUT`/`WD_BASE` pair is decoded and baked here and the `BAKE` line vanilla
//! printed must be reproduced verbatim.

use std::path::PathBuf;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{LoadError, McDataSource, WorldgenRegistries};
use mc_worldgen::densityfunction::codec::LoadResult;
use mc_worldgen::world_preset::{
    DimensionTypes, Holder, LevelStem, Lifecycle, SpecialWorldProperty, WorldDimensions,
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
        // `DimensionType.DIRECT_CODEC` lives in the game crate (tested there against vanilla)
        Ok(Arc::new(json.clone()))
    }
}

fn fixture() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flat/world_dimensions.txt");
    std::fs::read_to_string(path).unwrap()
}

fn lifecycle(l: Lifecycle) -> &'static str {
    match l {
        Lifecycle::Stable => "Stable",
        Lifecycle::Experimental => "Experimental",
    }
}

fn bake_line(
    name: &str,
    input: &Value,
    base: Option<&Value>,
    registries: &Arc<WorldgenRegistries>,
    biomes: &Arc<BiomeRegistry>,
) -> String {
    let types = RawTypes(registries.clone());
    let Ok(world) = WorldDimensions::from_json(input, registries, biomes, &types) else {
        return format!("BAKE {name} ERR");
    };
    let mut base_stems = Vec::new();
    for (key, stem) in base.and_then(Value::as_object).into_iter().flatten() {
        let stem =
            LevelStem::from_json(stem, registries, biomes, &types).expect("base stems are valid");
        base_stems.push((Identifier::read(key).unwrap(), Arc::new(stem)));
    }
    let complete = world.bake(&base_stems);
    let special = match complete.special_world_property {
        SpecialWorldProperty::None => "NONE",
        SpecialWorldProperty::Flat => "FLAT",
        SpecialWorldProperty::Debug => "DEBUG",
    };
    let levels: String = world.levels().iter().map(|k| format!("{k},")).collect();
    let mut line = format!(
        "BAKE {name} OK special={special} lifecycle={} levels={levels} debug={} entries=",
        lifecycle(complete.lifecycle),
        world.is_debug()
    );
    for entry in &complete.dimensions {
        let ty = match &entry.stem.dimension_type {
            Holder::Reference(id, _) => id.to_string(),
            Holder::Direct(_) => "<inline>".to_string(),
        };
        let possible = entry
            .stem
            .generator
            .as_chunk_generator()
            .biome_source()
            .possible_biomes();
        let inline = possible
            .iter()
            .filter(|b| BiomeRegistry::is_inline(**b))
            .count();
        // a baked registration never carries a known pack
        line.push_str(&format!(
            "{}/{}/false/{ty}/{}/{}/{inline};",
            entry.key,
            lifecycle(entry.lifecycle),
            entry.stem.generator.type_name(),
            possible.len()
        ));
    }
    line
}

#[test]
fn world_dimensions_bake_matches_vanilla() {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(McDataSource)));
    let biomes = Arc::new(BiomeRegistry::load(&registries).unwrap());
    let text = fixture();
    let mut lines = text.lines();
    let mut cases = 0;
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("WD_INPUT ") else {
            continue;
        };
        let (name, input) = rest.split_once(' ').unwrap();
        let base = lines
            .next()
            .unwrap()
            .strip_prefix(&format!("WD_BASE {name} "))
            .unwrap();
        let base: Option<Value> = (base != "none").then(|| serde_json::from_str(base).unwrap());
        let want = lines.next().unwrap();
        let input: Value = serde_json::from_str(input).unwrap();
        assert_eq!(
            bake_line(name, &input, base.as_ref(), &registries, &biomes),
            want,
            "{name}"
        );
        cases += 1;
    }
    assert_eq!(cases, 49);
}
