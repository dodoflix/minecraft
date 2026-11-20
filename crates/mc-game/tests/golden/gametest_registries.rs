//! The game test registries against the vanilla registry report (`ref/extracted/registries.json`): every
//! `test_environment_definition_type`, `test_instance_type` and `test_function` entry of vanilla has a decoder or
//! implementation here, nothing is accepted that vanilla does not know, and the `/test` tree has no `export*` literal
//! (those exist only when `SharedConstants.IS_RUNNING_IN_IDE` is true).

use std::collections::BTreeSet;
use std::path::Path;

use mc_core::ident::Identifier;
use mc_game::gametest::builtin_test_functions::TEST_FUNCTIONS;
use mc_game::gametest::game_test_instance::decode_instance;
use mc_game::gametest::test_environment_definition::decode_definition;
use mc_game::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
use serde_json::{Value, json};

use crate::command_common::vanilla_commands;

struct Everything;

impl RegistryLookup for Everything {
    fn contains(&self, _registry: &str, _id: &Identifier) -> Lookup {
        Lookup::Yes
    }
    fn tag_exists(&self, _registry: &str, _tag: &Identifier) -> Lookup {
        Lookup::Yes
    }
}

/// The entries of a vanilla registry, in registry order.
fn entries(registry: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/extracted/registries.json");
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("registries.json"))
            .expect("registries.json is JSON");
    report[registry]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("{registry} is not in the registry report"))
        .iter()
        .map(|e| e.as_str().expect("entry name").to_owned())
        .collect()
}

/// The smallest valid JSON of every `test_environment_definition_type`.
fn environment(kind: &str) -> Value {
    match kind {
        "minecraft:all_of" => json!({"type": kind, "definitions": []}),
        "minecraft:clock_time" => json!({"type": kind, "clock": "minecraft:overworld", "time": 0}),
        "minecraft:difficulty" => json!({"type": kind, "difficulty": "easy"}),
        "minecraft:function" => json!({"type": kind}),
        "minecraft:game_rules" => json!({"type": kind, "rules": {}}),
        "minecraft:timeline_attributes" => json!({"type": kind, "timelines": []}),
        "minecraft:weather" => json!({"type": kind, "weather": "clear"}),
        other => panic!(
            "vanilla has a new test environment type {other}: add its decoder and a case here"
        ),
    }
}

#[test]
fn every_test_environment_type_decodes() {
    let ctx = RegistryDecodeContext::new(&Everything);
    let kinds = entries("minecraft:test_environment_definition_type");
    assert_eq!(kinds.len(), 7, "{kinds:?}");
    for kind in &kinds {
        decode_definition(&environment(kind), &ctx).unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
    let err = decode_definition(&json!({"type": "minecraft:nope"}), &ctx)
        .err()
        .expect("an unknown type is refused");
    assert!(err.contains("test_environment_definition_type"), "{err}");
}

#[test]
fn every_test_instance_type_decodes() {
    let ctx = RegistryDecodeContext::new(&Everything);
    let kinds = entries("minecraft:test_instance_type");
    assert_eq!(kinds, ["minecraft:block_based", "minecraft:function"]);
    for kind in &kinds {
        let mut json = json!({
            "type": kind, "environment": "minecraft:default", "structure": "minecraft:empty", "max_ticks": 1
        });
        if kind == "minecraft:function" {
            json["function"] = json!("minecraft:always_pass");
        }
        decode_instance(&json, &ctx).unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
    let err = decode_instance(&json!({"type": "minecraft:nope"}), &ctx)
        .err()
        .expect("an unknown type is refused");
    assert!(err.contains("test_instance_type"), "{err}");
}

#[test]
fn test_functions_are_the_vanilla_ones() {
    let ours: BTreeSet<&str> = TEST_FUNCTIONS.iter().map(|f| f.id).collect();
    let vanilla = entries("minecraft:test_function");
    assert_eq!(vanilla, ["minecraft:always_pass"]);
    assert_eq!(ours, vanilla.iter().map(String::as_str).collect());
}

#[test]
fn the_test_command_has_no_export_literals() {
    assert!(!mc_game::gametest::IS_RUNNING_IN_IDE);
    let commands = vanilla_commands();
    let root = commands.get_dispatcher().get_root();
    let test = root
        .get_children()
        .into_iter()
        .find(|c| c.get_name() == "test")
        .expect("test is registered");
    let names: Vec<String> = test
        .get_children()
        .iter()
        .map(|c| c.get_name().to_owned())
        .collect();
    assert_eq!(names.len(), 17, "{names:?}");
    assert!(names.iter().all(|n| !n.starts_with("export")), "{names:?}");
}
