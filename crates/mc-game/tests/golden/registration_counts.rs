//! Guards the linkme distributed-slice registrations: every crate that contributes registrations must stay linked into
//! the final binary (an unreferenced crate's `#[distributed_slice]` statics are silently dropped by the linker).

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

macro_rules! counts {
    ($($s:ident),* $(,)?) => { vec![$((stringify!($s), $s.len())),*] };
}

#[test]
fn linkme_registration_counts_are_stable() {
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
    let rendered: String = actual.iter().map(|(n, c)| format!("{n}={c}\n")).collect();
    print!("{rendered}");
    assert_eq!(rendered, EXPECTED);
}

const EXPECTED: &str = "\
TRIGGER_TYPES=58\n\
COMMANDS=87\n\
ARGUMENT_TYPE_INFOS=62\n\
TEST_FUNCTIONS=1\n\
DYNAMIC_REGISTRY_DECODERS=40\n\
PLAY_HANDLERS=9\n\
PLAYER_SYSTEMS=1\n\
ITEM_SYSTEMS=1\n\
RELOAD_LISTENERS=3\n\
BLOCK_ENTITY_TYPES=44\n\
DISPENSE_BEHAVIORS=5\n\
ENTITY_FACTORIES=120\n\
SCENARIO_DUMP_EXTRA=4\n\
SCENARIO_FINALIZE_SPAWN=1\n\
SPAWN_PLACEMENTS=27\n\
ITEM_CLASSES=47\n\
ITEMS=0\n\
BLOCK_CLASSES=231\n\
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
CUSTOM_SPAWNERS=1\n\
";
