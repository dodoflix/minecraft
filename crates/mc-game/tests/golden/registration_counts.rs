//! Guards the linkme distributed-slice registrations: every crate that contributes registrations must stay linked into
//! the final binary (an unreferenced crate's `#[distributed_slice]` statics are silently dropped by the linker). The
//! recorded counts are floors: a dropped crate lowers a count, while new registrations may raise it.
//!
//! The second test pins the Rust registries against the vanilla registry report: a registry with a table here has exactly
//! as many entries as vanilla, and every other registry of the report is listed with the reason it has no countable table.

use mc_game::advancements::triggers::TRIGGER_TYPES;
use mc_game::commands::COMMANDS;
use mc_game::commands::synchronization::ARGUMENT_TYPE_INFOS;
use mc_game::gametest::builtin_test_functions::TEST_FUNCTIONS;
use mc_game::registry::decoders::DYNAMIC_REGISTRY_DECODERS;
use mc_game::server::network::game_packets::hooks::{ITEM_SYSTEMS, PLAYER_SYSTEMS, PLAY_HANDLERS};
use mc_game::server::reloadable_resources::RELOAD_LISTENERS;
use mc_game::world::block_entity::BLOCK_ENTITY_TYPES;
use mc_game::world::dispenser::DISPENSE_BEHAVIORS;
use mc_game::world::entity::factory::ENTITY_FACTORIES;
use mc_game::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, SCENARIO_FINALIZE_SPAWN};
use mc_game::world::entity::spawn_placements::SPAWN_PLACEMENTS;
use mc_game::world::item::{ITEMS, ITEM_CLASSES};
use mc_game::world::level::block::{BLOCKS, BLOCK_CLASSES};
use mc_game::world::level::levelgen::blockpredicates::BLOCK_PREDICATE_TYPES;
use mc_game::world::level::levelgen::feature::FEATURE_TYPES;
use mc_game::world::level::levelgen::feature::tree::{
    FEATURE_SIZE_TYPES, FOLIAGE_PLACER_TYPES, ROOT_PLACER_TYPES, TREE_DECORATOR_TYPES, TRUNK_PLACER_TYPES,
};
use mc_game::world::level::levelgen::placement::PLACEMENT_MODIFIER_TYPES;
use mc_game::world::level::levelgen::stateproviders::BLOCK_STATE_PROVIDER_TYPES;
use mc_game::world::level::levelgen::structure::piece::STRUCTURE_PIECE_TYPES;
use mc_game::world::level::levelgen::structure::pools::structure_pool_element::STRUCTURE_POOL_ELEMENT_TYPES;
use mc_game::world::level::levelgen::structure::structure::STRUCTURE_TYPES;
use mc_game::world::level::levelgen::structure::templatesystem::pos_rule_test::POS_RULE_TEST_TYPES;
use mc_game::world::level::levelgen::structure::templatesystem::rule::blockentity::rule_block_entity_modifier::RULE_BLOCK_ENTITY_MODIFIER_TYPES;
use mc_game::world::level::levelgen::structure::templatesystem::rule_test::RULE_TEST_TYPES;
use mc_game::world::level::levelgen::structure::templatesystem::structure_processor::STRUCTURE_PROCESSOR_TYPES;
use mc_game::world::level::material::FLUIDS;
use mc_game::world::level::spawner::custom_spawner::CUSTOM_SPAWNERS;
use mc_game::advancements::predicates::entity::entity_predicate::entity_sub_predicate_type_count;
use mc_game::server::level::TicketType;
use mc_game::world::entity::ai::memory::memory_module_type;
use mc_game::world::entity::ai::sensing::sensor_type::SENSOR_TYPES;
use mc_game::world::entity::schedule::ACTIVITIES;
use mc_game::world::item::enchantment::enchantment_effect_components::EffectComponent;
use mc_game::world::item::slot::slot_source_type_count;
use mc_game::world::level::storage::loot::functions::function_type_count;
use mc_game::world::level::storage::loot::parameters::LootContextParamSets;
use mc_game::world::level::storage::loot::predicates::condition_type_count;
use mc_game::world::level::storage::loot::providers::nbt::nbt_provider_type_count;
use mc_game::world::level::storage::loot::providers::number::{
    float_provider_type_count, int_provider_type_count,
};
use mc_game::world::level::storage::loot::providers::score::score_provider_type_count;
use mc_world::status::ChunkStatus;
use std::collections::BTreeSet;
use std::path::Path;

macro_rules! counts {
    ($($s:ident),* $(,)?) => { vec![$((stringify!($s), $s.len())),*] };
}

#[test]
fn linkme_registration_counts_do_not_drop() {
    let actual = counts![
        TRIGGER_TYPES,
        COMMANDS,
        ARGUMENT_TYPE_INFOS,
        TEST_FUNCTIONS,
        DYNAMIC_REGISTRY_DECODERS,
        PLAY_HANDLERS,
        PLAYER_SYSTEMS,
        ITEM_SYSTEMS,
        RELOAD_LISTENERS,
        BLOCK_ENTITY_TYPES,
        DISPENSE_BEHAVIORS,
        ENTITY_FACTORIES,
        SCENARIO_DUMP_EXTRA,
        SCENARIO_FINALIZE_SPAWN,
        SPAWN_PLACEMENTS,
        ITEM_CLASSES,
        ITEMS,
        BLOCK_CLASSES,
        BLOCKS,
        BLOCK_PREDICATE_TYPES,
        FEATURE_TYPES,
        TRUNK_PLACER_TYPES,
        FOLIAGE_PLACER_TYPES,
        ROOT_PLACER_TYPES,
        TREE_DECORATOR_TYPES,
        FEATURE_SIZE_TYPES,
        PLACEMENT_MODIFIER_TYPES,
        BLOCK_STATE_PROVIDER_TYPES,
        STRUCTURE_PIECE_TYPES,
        STRUCTURE_POOL_ELEMENT_TYPES,
        STRUCTURE_TYPES,
        POS_RULE_TEST_TYPES,
        RULE_BLOCK_ENTITY_MODIFIER_TYPES,
        RULE_TEST_TYPES,
        STRUCTURE_PROCESSOR_TYPES,
        FLUIDS,
        CUSTOM_SPAWNERS,
    ];
    let expected: Vec<(&str, usize)> = EXPECTED
        .lines()
        .map(|l| {
            let (n, c) = l.split_once('=').expect("NAME=count");
            (n, c.parse().expect("count"))
        })
        .collect();
    assert_eq!(
        actual.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        expected.iter().map(|(n, _)| *n).collect::<Vec<_>>()
    );
    let dropped: Vec<String> = actual
        .iter()
        .zip(&expected)
        .filter(|((_, a), (_, e))| a < e)
        .map(|((n, a), (_, e))| format!("{n}: {a} < {e}"))
        .collect();
    assert!(
        dropped.is_empty(),
        "registrations dropped (a crate is no longer linked?): {dropped:?}"
    );
}

const EXPECTED: &str = "\
TRIGGER_TYPES=58\n\
COMMANDS=87\n\
ARGUMENT_TYPE_INFOS=62\n\
TEST_FUNCTIONS=1\n\
DYNAMIC_REGISTRY_DECODERS=40\n\
PLAY_HANDLERS=10\n\
PLAYER_SYSTEMS=1\n\
ITEM_SYSTEMS=1\n\
RELOAD_LISTENERS=3\n\
BLOCK_ENTITY_TYPES=49\n\
DISPENSE_BEHAVIORS=19\n\
ENTITY_FACTORIES=160\n\
SCENARIO_DUMP_EXTRA=62\n\
SCENARIO_FINALIZE_SPAWN=1\n\
SPAWN_PLACEMENTS=33\n\
ITEM_CLASSES=61\n\
ITEMS=0\n\
BLOCK_CLASSES=239\n\
BLOCKS=2\n\
BLOCK_PREDICATE_TYPES=16\n\
FEATURE_TYPES=58\n\
TRUNK_PLACER_TYPES=10\n\
FOLIAGE_PLACER_TYPES=12\n\
ROOT_PLACER_TYPES=1\n\
TREE_DECORATOR_TYPES=11\n\
FEATURE_SIZE_TYPES=2\n\
PLACEMENT_MODIFIER_TYPES=18\n\
BLOCK_STATE_PROVIDER_TYPES=10\n\
STRUCTURE_PIECE_TYPES=56\n\
STRUCTURE_POOL_ELEMENT_TYPES=5\n\
STRUCTURE_TYPES=16\n\
POS_RULE_TEST_TYPES=3\n\
RULE_BLOCK_ENTITY_MODIFIER_TYPES=4\n\
RULE_TEST_TYPES=10\n\
STRUCTURE_PROCESSOR_TYPES=11\n\
FLUIDS=4\n\
CUSTOM_SPAWNERS=5\n\
";

/// Registries without a countable Rust table, with the reason; together with the guarded ones in
/// [`rust_registry_counts`] they are every registry of the vanilla report.
const SKIPPED: &[(&str, &str)] = &[
    ("attribute", NAMES_ONLY),
    ("attribute_type", NAMES_ONLY),
    ("block", NAMES_ONLY),
    ("consume_effect_type", BY_MATCH),
    ("creative_mode_tab", NAMES_ONLY),
    ("custom_stat", NAMES_ONLY),
    ("data_component_predicate_type", BY_MATCH),
    ("debug_subscription", NAMES_ONLY),
    ("dialog_action_type", BY_MATCH),
    ("dialog_body_type", BY_MATCH),
    ("dialog_type", BY_MATCH),
    ("enchantment_entity_effect_type", BY_MATCH),
    ("enchantment_level_based_value_type", BY_MATCH),
    ("enchantment_location_based_effect_type", BY_MATCH),
    ("enchantment_provider_type", BY_MATCH),
    ("enchantment_value_effect_type", BY_MATCH),
    ("environment_attribute", NAMES_ONLY),
    ("float_provider_type", BY_MATCH),
    ("game_event", NAMES_ONLY),
    ("game_rule", NAMES_ONLY),
    ("height_provider_type", BY_MATCH),
    ("incoming_rpc_methods", NAMES_ONLY),
    ("input_control_type", NAMES_ONLY),
    ("int_provider_type", BY_MATCH),
    ("item", NAMES_ONLY),
    ("loot_pool_entry_type", BY_MATCH),
    ("map_decoration_type", NAMES_ONLY),
    ("menu", NAMES_ONLY),
    ("mob_effect", NAMES_ONLY),
    ("number_format_type", BY_MATCH),
    ("outgoing_rpc_methods", NAMES_ONLY),
    ("particle_type", BY_MATCH),
    ("permission_check_type", NAMES_ONLY),
    ("permission_type", NAMES_ONLY),
    ("point_of_interest_type", NAMES_ONLY),
    ("position_source_type", BY_MATCH),
    ("potion", NAMES_ONLY),
    ("recipe_book_category", NAMES_ONLY),
    ("recipe_display", BY_MATCH),
    ("recipe_serializer", BY_MATCH),
    ("recipe_type", BY_MATCH),
    ("slot_display", BY_MATCH),
    ("sound_event", NAMES_ONLY),
    ("spawn_condition_type", LOCAL_TABLE),
    ("stat_type", NAMES_ONLY),
    ("test_environment_definition_type", BY_MATCH),
    ("test_instance_type", BY_MATCH),
    ("villager_profession", NAMES_ONLY),
    ("villager_type", NAMES_ONLY),
    ("worldgen/biome_source", BY_MATCH),
    ("worldgen/carver_type", BY_MATCH),
    ("worldgen/chunk_generator", BY_MATCH),
    ("worldgen/density_function_type", BY_MATCH),
    ("worldgen/material_condition_type", BY_MATCH),
    ("worldgen/material_rule_type", BY_MATCH),
    ("worldgen/pool_alias_binding", BY_MATCH),
    ("worldgen/structure_placement", BY_MATCH),
];

const NAMES_ONLY: &str = "only the generated name table of mc-data (its report test pins the size); Rust has no table of its own";
const BY_MATCH: &str = "implemented by a `match` over the type name or an enum without a variant list; no table to count";
const LOCAL_TABLE: &str =
    "the type table is local to its codec function and cannot be counted from outside";

/// `(registry, entries on the Rust side)` for every registry that has a countable table; `minecraft:` is implied.
fn rust_registry_counts() -> Vec<(&'static str, usize)> {
    vec![
        ("activity", ACTIVITIES.len()),
        ("sensor_type", SENSOR_TYPES.len()),
        ("memory_module_type", memory_module_type::REGISTRY.len()),
        ("ticket_type", TicketType::ALL.len()),
        ("chunk_status", ChunkStatus::ALL.len()),
        (
            "enchantment_effect_component_type",
            EffectComponent::ALL.len(),
        ),
        ("trigger_type", TRIGGER_TYPES.len()),
        ("command_argument_type", ARGUMENT_TYPE_INFOS.len()),
        ("block_entity_type", BLOCK_ENTITY_TYPES.len()),
        ("block_predicate_type", BLOCK_PREDICATE_TYPES.len()),
        ("test_function", TEST_FUNCTIONS.len()),
        // every entity type but the player has a registered factory
        ("entity_type", ENTITY_FACTORIES.len() + 1),
        // the empty fluid has no registration
        ("fluid", FLUIDS.len() + 1),
        ("worldgen/feature_type", FEATURE_TYPES.len()),
        ("worldgen/trunk_placer_type", TRUNK_PLACER_TYPES.len()),
        ("worldgen/foliage_placer_type", FOLIAGE_PLACER_TYPES.len()),
        ("worldgen/root_placer_type", ROOT_PLACER_TYPES.len()),
        ("worldgen/tree_decorator_type", TREE_DECORATOR_TYPES.len()),
        ("worldgen/feature_size_type", FEATURE_SIZE_TYPES.len()),
        (
            "worldgen/placement_modifier_type",
            PLACEMENT_MODIFIER_TYPES.len(),
        ),
        (
            "worldgen/block_state_provider_type",
            BLOCK_STATE_PROVIDER_TYPES.len(),
        ),
        ("worldgen/structure_piece", STRUCTURE_PIECE_TYPES.len()),
        (
            "worldgen/structure_pool_element",
            STRUCTURE_POOL_ELEMENT_TYPES.len(),
        ),
        ("worldgen/structure_type", STRUCTURE_TYPES.len()),
        (
            "worldgen/structure_processor",
            STRUCTURE_PROCESSOR_TYPES.len(),
        ),
        ("pos_rule_test", POS_RULE_TEST_TYPES.len()),
        (
            "rule_block_entity_modifier",
            RULE_BLOCK_ENTITY_MODIFIER_TYPES.len(),
        ),
        ("rule_test", RULE_TEST_TYPES.len()),
        ("loot_function_type", function_type_count()),
        ("loot_condition_type", condition_type_count()),
        ("loot_nbt_provider_type", nbt_provider_type_count()),
        ("loot_score_provider_type", score_provider_type_count()),
        ("context_int_provider_type", int_provider_type_count()),
        ("context_float_provider_type", float_provider_type_count()),
        ("slot_source_type", slot_source_type_count()),
        (
            "entity_sub_predicate_type",
            entity_sub_predicate_type_count(),
        ),
        (
            "data_component_type",
            mc_protocol::components::registry::defined().count(),
        ),
        ("context_key_set", LootContextParamSets::ALL.len()),
    ]
}

#[test]
fn rust_registry_counts_match_the_vanilla_report() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/generated/reports/registries.json");
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("registries.json"))
            .expect("registries.json is JSON");
    let report = report.as_object().expect("the report is an object");
    let vanilla = |name: &str| {
        report
            .get(&format!("minecraft:{name}"))
            .and_then(|r| r["entries"].as_object())
            .map(|entries| entries.len())
    };

    let counts = rust_registry_counts();
    let guarded: BTreeSet<&str> = counts.iter().map(|&(name, _)| name).collect();
    let skipped: BTreeSet<&str> = SKIPPED.iter().map(|&(name, _)| name).collect();
    assert_eq!(guarded.len(), counts.len(), "a registry is guarded twice");
    assert_eq!(skipped.len(), SKIPPED.len(), "a registry is skipped twice");

    let mut problems = Vec::new();
    for &(name, rust) in &counts {
        match vanilla(name) {
            Some(v) if v == rust => {}
            Some(v) => problems.push(format!("{name}: Rust has {rust}, vanilla has {v}")),
            None => problems.push(format!("{name}: guarded but not in the report")),
        }
    }
    for name in &skipped {
        if guarded.contains(name) {
            problems.push(format!("{name}: both guarded and skipped"));
        } else if vanilla(name).is_none() {
            problems.push(format!("{name}: skipped but not in the report"));
        }
    }
    for key in report.keys() {
        let name = key.strip_prefix("minecraft:").unwrap_or(key);
        if !guarded.contains(name) && !skipped.contains(name) {
            problems.push(format!(
                "{name}: a vanilla registry that is neither guarded nor skipped"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "registry entry counts drifted from vanilla:\n{}",
        problems.join("\n")
    );
}
