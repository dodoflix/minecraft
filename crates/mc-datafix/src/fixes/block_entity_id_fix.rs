//! `net.minecraft.util.datafix.fixes.BlockEntityIdFix`.

use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_core::math::java::java_string_hash;
use std::sync::LazyLock;

/// `BlockEntityIdFix.ID_MAP` in the order the `put`s were made.
const ID_MAP_PUTS: [(&str, &str); 23] = [
    ("Airportal", "minecraft:end_portal"),
    ("Banner", "minecraft:banner"),
    ("Beacon", "minecraft:beacon"),
    ("Cauldron", "minecraft:brewing_stand"),
    ("Chest", "minecraft:chest"),
    ("Comparator", "minecraft:comparator"),
    ("Control", "minecraft:command_block"),
    ("DLDetector", "minecraft:daylight_detector"),
    ("Dropper", "minecraft:dropper"),
    ("EnchantTable", "minecraft:enchanting_table"),
    ("EndGateway", "minecraft:end_gateway"),
    ("EnderChest", "minecraft:ender_chest"),
    ("FlowerPot", "minecraft:flower_pot"),
    ("Furnace", "minecraft:furnace"),
    ("Hopper", "minecraft:hopper"),
    ("MobSpawner", "minecraft:mob_spawner"),
    ("Music", "minecraft:noteblock"),
    ("Piston", "minecraft:piston"),
    ("RecordPlayer", "minecraft:jukebox"),
    ("Sign", "minecraft:sign"),
    ("Skull", "minecraft:skull"),
    ("Structure", "minecraft:structure_block"),
    ("Trap", "minecraft:dispenser"),
];

/// `BlockEntityIdFix.ID_MAP` (old id, new id) in `HashMap` iteration order.
pub fn id_map() -> &'static [(&'static str, &'static str)] {
    static ID_MAP: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
        hash_map_ordered(
            ID_MAP_PUTS.to_vec(),
            |(old_id, _)| java_string_hash(old_id),
            HASH_MAP_DEFAULT_CAPACITY,
        )
    });
    &ID_MAP
}
