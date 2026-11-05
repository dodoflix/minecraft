//! Codec tests of the material rules / conditions: accept/reject outcomes against the vanilla codecs
//! (`tests/oracle/MaterialCodecOracle.java`), all vanilla entries, and reference cycles.

use std::path::PathBuf;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{ClosureDataSource, FsDataSource, WorldgenRegistries};
use mc_worldgen::material::{
    MATERIAL_CONDITION_REGISTRY, MATERIAL_RULE_REGISTRY, MaterialCondition, MaterialRule,
};
use serde_json::{Value, json};

fn data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data")
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn vanilla() -> (Arc<WorldgenRegistries>, BiomeRegistry) {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(FsDataSource::new(data_root()))));
    let biomes = BiomeRegistry::load(&registries).expect("biomes");
    (registries, biomes)
}

#[test]
fn codec_outcomes_match_vanilla() {
    let cases: Vec<Value> = serde_json::from_str(&fixture("material_codec_cases.json")).unwrap();
    let expected: std::collections::HashMap<String, bool> = fixture("material_codec.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (name, outcome) = l.rsplit_once(' ').unwrap();
            (name.to_owned(), outcome == "ok")
        })
        .collect();
    assert_eq!(expected.len(), cases.len());
    let (registries, biomes) = vanilla();
    let mut mismatches = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let result = match case["kind"].as_str().unwrap() {
            "rule" => MaterialRule::from_json(&case["json"], &registries, &biomes).map(drop),
            _ => MaterialCondition::from_json(&case["json"], &registries, &biomes).map(drop),
        };
        let ours = result.is_ok();
        // Documented deviation: an unknown noise key fails at decode time (vanilla: when first compiled).
        let rust_differs = case.get("rust_differs").and_then(Value::as_bool).unwrap_or(false);
        let want = if rust_differs { false } else { expected[name] };
        if ours != want {
            mismatches.push(format!(
                "{name}: vanilla {} ours {} ({}) json {}",
                expected[name],
                ours,
                result.err().map(|e| e.to_string()).unwrap_or_default(),
                case["json"]
            ));
        }
    }
    assert!(mismatches.is_empty(), "{} mismatches:\n{}", mismatches.len(), mismatches.join("\n"));
}

#[test]
fn all_vanilla_entries_and_noise_settings_decode() {
    let (registries, biomes) = vanilla();
    let rules = registries.list(MATERIAL_RULE_REGISTRY);
    let conditions = registries.list(MATERIAL_CONDITION_REGISTRY);
    assert!(rules.len() >= 20 && conditions.len() == 8, "{} rules, {} conditions", rules.len(), conditions.len());
    for id in &rules {
        MaterialRule::from_json(&Value::String(id.to_string()), &registries, &biomes)
            .unwrap_or_else(|e| panic!("rule {id}: {e}"));
    }
    for id in &conditions {
        MaterialCondition::from_json(&Value::String(id.to_string()), &registries, &biomes)
            .unwrap_or_else(|e| panic!("condition {id}: {e}"));
    }
    for name in ["overworld", "large_biomes", "amplified", "nether", "end", "caves", "floating_islands"] {
        let settings = registries
            .noise_settings(&Identifier::with_default_namespace(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        MaterialRule::from_json(&settings.material_rule, &registries, &biomes)
            .unwrap_or_else(|e| panic!("{name} material_rule: {e}"));
    }
}

fn in_memory(entries: Vec<(&'static str, &'static str, Value)>) -> WorldgenRegistries {
    WorldgenRegistries::new(Arc::new(ClosureDataSource::new(move |registry: &str, id: &Identifier| {
        entries
            .iter()
            .find(|(r, p, _)| *r == registry && *p == id.path())
            .map(|(_, _, v)| v.clone())
    })))
}

#[test]
fn reference_cycles_are_errors() {
    let (_, biomes) = vanilla();
    let registries = in_memory(vec![
        ("worldgen/material_rule", "a", json!({"type": "minecraft:sequence", "sequence": ["minecraft:b"]})),
        ("worldgen/material_rule", "b", json!({"type": "minecraft:condition", "if_true": {"type": "minecraft:steep"}, "then_run": "minecraft:a"})),
        ("worldgen/material_rule", "self", json!("minecraft:self")),
        ("worldgen/material_condition", "c1", json!({"type": "minecraft:not", "invert": "minecraft:c2"})),
        ("worldgen/material_condition", "c2", json!({"type": "minecraft:not", "invert": "minecraft:c1"})),
        ("worldgen/material_condition", "ok", json!({"type": "minecraft:hole"})),
    ]);
    for id in ["minecraft:a", "minecraft:b"] {
        let err = MaterialRule::from_json(&json!(id), &registries, &biomes).unwrap_err().to_string();
        assert!(err.contains("reference cycle"), "{err}");
    }
    assert!(MaterialRule::from_json(&json!("minecraft:self"), &registries, &biomes).is_err());
    let err = MaterialCondition::from_json(&json!("minecraft:c1"), &registries, &biomes).unwrap_err().to_string();
    assert!(err.contains("reference cycle"), "{err}");
    // a failed cyclic load leaves the registry usable
    assert!(MaterialCondition::from_json(&json!("minecraft:ok"), &registries, &biomes).is_ok());
    assert!(MaterialRule::from_json(&json!("minecraft:a"), &registries, &biomes).is_err());
}
