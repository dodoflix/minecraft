//! The decode problems of a block entity that is loaded from a chunk (`BlockEntity.loadStatic`) are logged by the
//! `BlockEntity` logger under the path of the block entity, like the real server logs them (texts recorded from it).

use crate::common;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::Block;
use mc_game::world::block_entity::{create_block_entity, load_static_data};
use mc_nbt::snbt;

fn logs_of(block: &str, ty: BlockEntityType, pos: BlockPos, tag: &str) -> Vec<String> {
    common::init_logger();
    let state = Block::by_name(block).expect("block").default_state();
    let mut entity = create_block_entity(ty, pos, state).expect("implementation");
    common::take_logs();
    load_static_data(&mut *entity, &snbt::parse_compound(tag).expect("snbt"));
    common::take_logs()
}

#[test]
fn a_container_item_that_does_not_decode_is_reported_with_its_index() {
    let logs = logs_of(
        "chest",
        BlockEntityType::CHEST,
        BlockPos::new(1, 4, 1),
        r#"{Items:[{Slot:4b,components:{"minecraft:entity_data":{id:"Pig"}},count:1,id:"minecraft:wither_spawn_egg"}]}"#,
    );
    assert_eq!(
        logs,
        [concat!(
            "WARN: [net.minecraft.world.level.block.entity.BlockEntity] Serialization errors:\n",
            "minecraft:chest // net.minecraft.world.level.block.entity.ChestBlockEntity@BlockPos{x=1, y=4, z=1}: ",
            r#"Failed to decode value '{Slot:4b,components:{"minecraft:entity_data":{id:"Pig"}},count:1,id:"minecraft:wither_spawn_egg"}' "#,
            "from field 'Items' at index 0': Not a valid resource location: Pig ",
            "Non [a-z0-9/._-] character in path of location: minecraft:Pig ",
            r#"missed input: {"minecraft:entity_data":{id:"Pig"}}"#,
        )]
    );
}

#[test]
fn a_spawner_entry_without_weight_and_data_is_reported() {
    let logs = logs_of(
        "spawner",
        BlockEntityType::MOB_SPAWNER,
        BlockPos::new(7, 4, 1),
        "{SpawnPotentials:[{Entity:{},Weight:1}]}",
    );
    assert_eq!(
        logs,
        [concat!(
            "WARN: [net.minecraft.world.level.block.entity.BlockEntity] Serialization errors:\n",
            "minecraft:mob_spawner // net.minecraft.world.level.block.entity.SpawnerBlockEntity@BlockPos{x=7, y=4, z=1}: ",
            "Failed to decode value '[{Entity:{},Weight:1}]' from field 'SpawnPotentials': ",
            "No key weight in MapLike[{Entity:{},Weight:1}]; No key data in MapLike[{Entity:{},Weight:1}]",
        )]
    );
}
