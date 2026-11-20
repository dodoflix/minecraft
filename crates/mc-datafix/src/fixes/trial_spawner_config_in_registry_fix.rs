//! `TrialSpawnerConfigInRegistryFix`: replaces the inline `normal_config` / `ominous_config` of a trial spawner that
//! match a vanilla trial chamber configuration with the key of that configuration in the registry.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::{CompoundTag, Tag, snbt};
use std::sync::LazyLock;

/// `new TrialSpawnerConfigInRegistryFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "TrialSpawnerConfigInRegistryFix",
        output_schema,
        false,
        "TrialSpawnerConfigInRegistryFix",
        &References::BLOCK_ENTITY,
        "minecraft:trial_spawner",
        |entity| {
            entity.update_remainder(|input| {
                let ops = input.ops();
                fix_tag(input.convert(&NBT)).convert(&ops)
            })
        },
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    let Some(normal_config) = input.get("normal_config").result() else {
        return input;
    };
    let Some(ominous_config) = input.get("ominous_config").result() else {
        return input;
    };
    let Some(registry_location) = VanillaTrialChambers::key_of(&normal_config, &ominous_config)
    else {
        return input;
    };
    input
        .set(
            "normal_config",
            input.create_string(format!("{registry_location}/normal")),
        )
        .set(
            "ominous_config",
            input.create_string(format!("{registry_location}/ominous")),
        )
}

/// `VanillaTrialChambers`.
struct VanillaTrialChambers;

/// `CONFIGS_TO_KEY`: `(normal, ominous)` keys with the identifier they map to. A `HashMap` is only ever read through
/// `get`, so the entries are a list where a repeated key replaces the earlier value.
static CONFIGS_TO_KEY: LazyLock<Vec<((Dynamic, Dynamic), String)>> = LazyLock::new(|| {
    let mut configs = Vec::new();
    for (location, normal_nbt, ominous_nbt) in REGISTRATIONS {
        register(&mut configs, location, normal_nbt, ominous_nbt);
    }
    configs
});

impl VanillaTrialChambers {
    fn key_of(normal: &Dynamic, ominous: &Dynamic) -> Option<&'static str> {
        CONFIGS_TO_KEY
            .iter()
            .find(|((n, o), _)| n == normal && o == ominous)
            .map(|(_, location)| location.as_str())
    }
}

fn put(configs: &mut Vec<((Dynamic, Dynamic), String)>, key: (Dynamic, Dynamic), location: &str) {
    match configs.iter_mut().find(|(k, _)| *k == key) {
        Some((_, existing)) => *existing = location.to_string(),
        None => configs.push((key, location.to_string())),
    }
}

fn register(
    configs: &mut Vec<((Dynamic, Dynamic), String)>,
    path: &str,
    normal_nbt: &str,
    ominous_nbt: &str,
) {
    let location = format!("minecraft:{path}");
    let normal_tag = parse(normal_nbt);
    let ominous_tag = parse(ominous_nbt);
    let mut ominous_merged_tag = normal_tag.copy();
    ominous_merged_tag.merge(&ominous_tag);
    let ominous_merged_tag_defaults_omitted = remove_defaults(ominous_merged_tag.copy());
    let dynamic_normal = as_dynamic(&normal_tag);
    put(
        configs,
        (dynamic_normal.clone(), as_dynamic(&ominous_tag)),
        &location,
    );
    put(
        configs,
        (dynamic_normal.clone(), as_dynamic(&ominous_merged_tag)),
        &location,
    );
    put(
        configs,
        (
            dynamic_normal,
            as_dynamic(&ominous_merged_tag_defaults_omitted),
        ),
        &location,
    );
}

fn as_dynamic(tag: &CompoundTag) -> Dynamic {
    Dynamic::new(NBT, Dyn::from(&Tag::Compound(tag.clone())))
}

fn parse(nbt: &str) -> CompoundTag {
    snbt::parse_compound(nbt).unwrap_or_else(|e| {
        panic!("java.lang.IllegalArgumentException: Failed to parse Trial Spawner NBT config: {nbt}: {e}")
    })
}

fn remove_defaults(mut tag: CompoundTag) -> CompoundTag {
    if tag.get_int_or("spawn_range", 0) == 4 {
        tag.remove("spawn_range");
    }
    if tag.get_float_or("total_mobs", 0.0) == 6.0 {
        tag.remove("total_mobs");
    }
    if tag.get_float_or("simultaneous_mobs", 0.0) == 2.0 {
        tag.remove("simultaneous_mobs");
    }
    if tag.get_float_or("total_mobs_added_per_player", 0.0) == 2.0 {
        tag.remove("total_mobs_added_per_player");
    }
    if tag.get_float_or("simultaneous_mobs_added_per_player", 0.0) == 1.0 {
        tag.remove("simultaneous_mobs_added_per_player");
    }
    if tag.get_int_or("ticks_between_spawn", 0) == 40 {
        tag.remove("ticks_between_spawn");
    }
    tag
}

/// The `register(location, normalNbt, ominousNbt)` calls of the static initializer, in order.
const REGISTRATIONS: [(&str, &str, &str); 14] = [
    (
        "trial_chamber/breeze",
        "{simultaneous_mobs: 1.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:breeze\"}}, weight: 1}], ticks_between_spawn: 20, total_mobs: 2.0f, total_mobs_added_per_player: 1.0f}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], simultaneous_mobs: 2.0f, total_mobs: 4.0f}",
    ),
    (
        "trial_chamber/melee/husk",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:husk\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], spawn_potentials: [{data: {entity: {id: \"minecraft:husk\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_melee\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/melee/spider",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:spider\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}],simultaneous_mobs: 4.0f, total_mobs: 12.0f}",
    ),
    (
        "trial_chamber/melee/zombie",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:zombie\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}],spawn_potentials: [{data: {entity: {id: \"minecraft:zombie\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_melee\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/ranged/poison_skeleton",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:bogged\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}],spawn_potentials: [{data: {entity: {id: \"minecraft:bogged\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_ranged\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/ranged/skeleton",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:skeleton\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], spawn_potentials: [{data: {entity: {id: \"minecraft:skeleton\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_ranged\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/ranged/stray",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:stray\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], spawn_potentials: [{data: {entity: {id: \"minecraft:stray\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_ranged\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/slow_ranged/poison_skeleton",
        "{simultaneous_mobs: 4.0f, simultaneous_mobs_added_per_player: 2.0f, spawn_potentials: [{data: {entity: {id: \"minecraft:bogged\"}}, weight: 1}], ticks_between_spawn: 160}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], spawn_potentials: [{data: {entity: {id: \"minecraft:bogged\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_ranged\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/slow_ranged/skeleton",
        "{simultaneous_mobs: 4.0f, simultaneous_mobs_added_per_player: 2.0f, spawn_potentials: [{data: {entity: {id: \"minecraft:skeleton\"}}, weight: 1}], ticks_between_spawn: 160}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], spawn_potentials: [{data: {entity: {id: \"minecraft:skeleton\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_ranged\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/slow_ranged/stray",
        "{simultaneous_mobs: 4.0f, simultaneous_mobs_added_per_player: 2.0f, spawn_potentials: [{data: {entity: {id: \"minecraft:stray\"}}, weight: 1}], ticks_between_spawn: 160}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}],spawn_potentials: [{data: {entity: {id: \"minecraft:stray\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_ranged\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/small_melee/baby_zombie",
        "{simultaneous_mobs: 2.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {IsBaby: 1b, id: \"minecraft:zombie\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], spawn_potentials: [{data: {entity: {IsBaby: 1b, id: \"minecraft:zombie\"}, equipment: {loot_table: \"minecraft:equipment/trial_chamber_melee\", slot_drop_chances: 0.0f}}, weight: 1}]}",
    ),
    (
        "trial_chamber/small_melee/cave_spider",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:cave_spider\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], simultaneous_mobs: 4.0f, total_mobs: 12.0f}",
    ),
    (
        "trial_chamber/small_melee/silverfish",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {id: \"minecraft:silverfish\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], simultaneous_mobs: 4.0f, total_mobs: 12.0f}",
    ),
    (
        "trial_chamber/small_melee/slime",
        "{simultaneous_mobs: 3.0f, simultaneous_mobs_added_per_player: 0.5f, spawn_potentials: [{data: {entity: {Size: 1, id: \"minecraft:slime\"}}, weight: 3}, {data: {entity: {Size: 2, id: \"minecraft:slime\"}}, weight: 1}], ticks_between_spawn: 20}",
        "{loot_tables_to_eject: [{data: \"minecraft:spawners/ominous/trial_chamber/key\", weight: 3}, {data: \"minecraft:spawners/ominous/trial_chamber/consumables\", weight: 7}], simultaneous_mobs: 4.0f, total_mobs: 12.0f}",
    ),
];
