//! Block entities of worldgen as raw NBT: the exact shape `BlockEntity.saveWithFullMetadata` produces for a block entity
//! that was just created by `setBlock` and then configured by a feature (`id`, `x`, `y`, `z`, the type's own fields and the
//! always-present empty `components` compound). Shared by every feature that fills a block entity.

use super::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
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
        "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel" | "minecraft:dispenser" | "minecraft:dropper" | "minecraft:hopper" | "minecraft:crafter" | "minecraft:shulker_box"
    )
}

/// The block entity type of a block (`EntityBlock.newBlockEntity` -> `BlockEntityType.isValid`).
fn block_entity_type_of(block: mc_data::block::Block) -> Option<BlockEntityType> {
    BlockEntityType::iter().find(|ty| ty.data().valid_blocks.contains(&block))
}

/// `new XBlockEntity(pos, state).saveWithFullMetadata()` for the types worldgen features configure (`None`: not modelled).
fn default_tag(id: &str, pos: BlockPos) -> Option<CompoundTag> {
    match id {
        "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel" => {
            let mut tag = base_tag(id, pos);
            tag.put("Items", ListTag::new());
            Some(tag)
        }
        "minecraft:mob_spawner" => {
            let mut tag = spawner_tag(pos, "");
            tag.remove("SpawnData");
            Some(tag)
        }
        "minecraft:end_gateway" => Some(end_gateway_tag(pos, None)),
        _ => None,
    }
}

/// `WorldGenRegion.getBlockEntity(pos)`: the chunk only holds raw NBT; a `DUMMY` entry (written by `setBlock` for block
/// entity states) turns into the block's fresh block entity the first time it is fetched. Returns the NBT of the (now
/// real) block entity, `None` without one (and for types whose fresh NBT is not modelled, which stay `DUMMY`).
pub fn get_block_entity_nbt(level: &mut dyn WorldGenLevel, pos: BlockPos) -> Option<CompoundTag> {
    let existing = level.get_block_entity_nbt(pos)?;
    if existing.get_string("id") != Some("DUMMY") {
        return Some(existing.clone());
    }
    let state = level.get_block_state(pos);
    let ty = block_entity_type_of(state.block())?;
    let tag = default_tag(ty.name(), pos)?;
    level.set_block_entity_nbt(tag.clone());
    Some(tag)
}

/// `RandomizableContainer.setBlockEntityLootTable(level, random, pos, lootTable)`: when the block entity at `pos` is a
/// randomizable container it gets the loot table and `random.nextLong()` as seed (`trySaveLootTable`: the seed is only
/// saved when non-zero, and a container with a loot table saves no `Items`).
pub fn set_block_entity_loot_table(level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, pos: BlockPos, loot_table: &str) {
    let Some(mut tag) = get_block_entity_nbt(level, pos) else {
        return;
    };
    if !tag.get_string("id").is_some_and(is_randomizable_container) {
        return;
    }
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
