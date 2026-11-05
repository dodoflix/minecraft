//! `Structure.StructureSettings.CODEC` (biomes, spawn overrides, step, terrain adaptation) against the vanilla codec:
//! `oracle/StructureSettingsOracle.java` over `fixtures/structure_settings_cases.in` (every vanilla structure + edge
//! cases).

extern crate mc_game_core as mc_game;
use mc_game::world::level::levelgen::codec::Fields;
use mc_game::world::level::levelgen::registries::{Cells, DecodeCtx, load_vanilla_data};
use mc_game::world::level::levelgen::structure::structure::{BoundingBoxType, StructureSettings};
use mc_worldgen::data::{FsDataSource, WorldgenDataSource};
use std::sync::Arc;

fn dump(s: &StructureSettings, biomes: &mc_worldgen::biome::BiomeRegistry) -> String {
    let ids: Vec<String> = s.biomes.iter().map(|b| biomes.key(*b).to_string()).collect();
    let mut overrides: Vec<String> = s
        .spawn_overrides
        .iter()
        .map(|(category, o)| {
            let spawns: Vec<String> = o
                .spawns
                .unwrap()
                .iter()
                .map(|w| {
                    let name = w.value.entity_type.data().name;
                    let name = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
                    format!("{name}*{}-{}@{}", w.value.count.min_inclusive(), w.value.count.max_inclusive(), w.weight)
                })
                .collect();
            let bb = match o.bounding_box {
                BoundingBoxType::Piece => "piece",
                BoundingBoxType::Structure => "full",
            };
            format!("{}:{bb}[{}]", category.data().serialized_name, spawns.join(","))
        })
        .collect();
    overrides.sort();
    let step = s.step.get_name();
    let adapt = format!("{:?}", s.terrain_adaptation);
    let adapt = match adapt.as_str() {
        "None" => "none",
        "Bury" => "bury",
        "BeardThin" => "beard_thin",
        "BeardBox" => "beard_box",
        "Encapsulate" => "encapsulate",
        other => panic!("{other}"),
    };
    format!("ok biomes={};overrides={};step={step};adapt={adapt}", ids.join(","), overrides.join("|"))
}

#[test]
fn structure_settings_match_vanilla() {
    let (worldgen, biomes, _) = load_vanilla_data();
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data")));
    let cells = Cells::new(source);
    let ctx = DecodeCtx { worldgen: &worldgen, biomes: &biomes, cells: &cells };
    let golden = include_str!("../../mc-game/tests/fixtures/structure_settings_cases.golden");
    let mut wrong = Vec::new();
    let mut total = 0;
    for line in golden.lines() {
        let (json, expected) = line.rsplit_once('\t').unwrap();
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let actual = match Fields::new(&value).and_then(|f| StructureSettings::decode(&f, &ctx)) {
            Ok(s) => dump(&s, &biomes),
            Err(_) => "err".to_owned(),
        };
        total += 1;
        if actual != expected {
            wrong.push(format!("{json}\n  vanilla: {expected}\n  ours:    {actual}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {total} cases differ:\n{}", wrong.len(), wrong.join("\n"));
}
