//! `TrialSpawnerConfigFix`: moves the spawner settings of a trial spawner into `normal_config`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_collections::hash_map_order;
use mc_dfu::Dynamic;

const KEYS_TO_MOVE: [&str; 9] = [
    "spawn_range",
    "total_mobs",
    "simultaneous_mobs",
    "total_mobs_added_per_player",
    "simultaneous_mobs_added_per_player",
    "ticks_between_spawn",
    "spawn_potentials",
    "loot_tables_to_eject",
    "items_to_drop_when_ominous",
];

/// `new TrialSpawnerConfigFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_write_read_fix::new(
        "TrialSpawnerConfigFix",
        output_schema,
        true,
        "Trial Spawner config tag fixer",
        &References::BLOCK_ENTITY,
        "minecraft:trial_spawner",
        move_to_config_tag,
    )
}

fn move_to_config_tag(mut input: Dynamic) -> Dynamic {
    let mut entries: Vec<(Dynamic, Dynamic)> = Vec::new();
    for key in KEYS_TO_MOVE {
        if let Some(value) = input.get(key).result() {
            entries.push((input.create_string(key), value));
            input = input.remove(key);
        }
    }
    if entries.is_empty() {
        return input;
    }
    // `new HashMap<>(keysToMove.size())` iterated by `createMap`
    let hashes: Vec<i32> = entries.iter().map(|(k, _)| k.hash_code()).collect();
    let order = hash_map_order(&hashes, KEYS_TO_MOVE.len());
    let mut slots: Vec<Option<(Dynamic, Dynamic)>> = entries.into_iter().map(Some).collect();
    let config = input.create_map(order.into_iter().filter_map(|i| slots[i].take()));
    input.set("normal_config", config)
}
