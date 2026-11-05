//! Block entities of worldgen as raw NBT: the exact shape `BlockEntity.saveWithFullMetadata` produces for a block entity
//! that was just created by `setBlock` and then configured by a feature (`id`, `x`, `y`, `z`, the type's own fields and the
//! always-present empty `components` compound). Shared by every feature that fills a block entity.

use super::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_nbt::{CompoundTag, ListTag};

/// `BlockEntity.saveWithFullMetadata` without the type specific fields.
pub fn base_tag(id: &str, pos: BlockPos) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", id);
    tag.put_int("x", pos.x());
    tag.put_int("y", pos.y());
    tag.put_int("z", pos.z());
    tag.put("components", CompoundTag::new());
    tag
}

/// Block entity types extending `RandomizableContainerBlockEntity` (`RandomizableContainer.setBlockEntityLootTable` targets).
fn is_randomizable_container(id: &str) -> bool {
    matches!(
        id,
        "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel" | "minecraft:dispenser" | "minecraft:dropper" | "minecraft:hopper" | "minecraft:crafter"
    ) || id == "minecraft:shulker_box"
}

/// `RandomizableContainer.setBlockEntityLootTable(level, random, pos, lootTable)`: when the block entity at `pos` is a
/// randomizable container it gets the loot table and `random.nextLong()` as seed (`trySaveLootTable`: the seed is only
/// saved when non-zero, and a container with a loot table saves no `Items`).
pub fn set_block_entity_loot_table(level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, pos: BlockPos, loot_table: &str) {
    let Some(existing) = level.get_block_entity_nbt(pos) else {
        return;
    };
    let Some(id) = existing.get_string("id").map(str::to_owned) else {
        return;
    };
    if !is_randomizable_container(&id) {
        return;
    }
    let mut tag = existing.clone();
    let seed = random.next_long();
    tag.remove("Items");
    tag.put_string("LootTable", loot_table);
    tag.remove("LootTableSeed");
    if seed != 0 {
        tag.put_long("LootTableSeed", seed);
    }
    level.set_block_entity_nbt(tag);
}

/// `SpawnerBlockEntity` right after `setEntityId(type, random)` on a fresh spawner (`BaseSpawner.save`).
pub fn spawner_tag(pos: BlockPos, entity_id: &str) -> CompoundTag {
    let mut tag = base_tag("minecraft:mob_spawner", pos);
    tag.put_short("Delay", 20);
    tag.put_short("MinSpawnDelay", 200);
    tag.put_short("MaxSpawnDelay", 800);
    tag.put_short("SpawnCount", 4);
    tag.put_short("MaxNearbyEntities", 6);
    tag.put_short("RequiredPlayerRange", 16);
    tag.put_short("SpawnRange", 4);
    let mut entity = CompoundTag::new();
    entity.put_string("id", entity_id);
    let mut spawn_data = CompoundTag::new();
    spawn_data.put("entity", entity);
    tag.put("SpawnData", spawn_data);
    tag.put("SpawnPotentials", ListTag::new());
    tag
}

/// `TheEndGatewayBlockEntity` (`Age`, and the exit when `setExitPosition(pos, exact)` was called).
pub fn end_gateway_tag(pos: BlockPos, exit: Option<(BlockPos, bool)>) -> CompoundTag {
    let mut tag = base_tag("minecraft:end_gateway", pos);
    tag.put_long("Age", 0);
    if let Some((exit_pos, exact)) = exit {
        tag.put_int_array("exit_portal", vec![exit_pos.x(), exit_pos.y(), exit_pos.z()]);
        if exact {
            tag.put_boolean("ExactTeleport", true);
        }
    }
    tag
}
