//! The decode problems of a block entity that is loaded from a chunk (`BlockEntity.loadStatic`) are logged by the
//! `BlockEntity` logger under the path of the block entity, like the real server logs them (texts recorded from it).

use crate::common;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::Block;
use mc_game::world::block_entity::{
    create_block_entity, create_block_entity_for_load, load_static_data,
};
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

/// A block entity of `ty` that a chunk holds on `block`: what `BlockEntity.loadStatic` logs for it.
fn create_logs(ty: BlockEntityType, block: &str, pos: BlockPos) -> (bool, Vec<String>) {
    common::init_logger();
    let state = Block::by_name(block).expect("block").default_state();
    common::take_logs();
    let failed = create_block_entity_for_load(ty, pos, state).is_err();
    (failed, common::take_logs())
}

fn normalised(logs: &[String]) -> Vec<String> {
    logs.iter()
        .map(|l| {
            let (head, rest) = l.split_once("BlockEntityType@").expect("type text");
            let (_, rest) = rest.split_once(' ').expect("hash end");
            format!("{head}BlockEntityType@<hash> {rest}")
        })
        .collect()
}

#[test]
fn a_block_entity_on_air_is_not_created() {
    let (failed, logs) = create_logs(BlockEntityType::CHEST, "air", BlockPos::new(33, -60, 5));
    assert!(failed);
    assert_eq!(
        normalised(&logs),
        [concat!(
            "ERROR: Failed to create block entity net.minecraft.world.level.block.entity.BlockEntityType@<hash> ",
            "for block Block{minecraft:air} at position BlockPos{x=33, y=-60, z=5} \n",
            "java.lang.IllegalStateException: Invalid block entity minecraft:chest // ",
            "net.minecraft.world.level.block.entity.ChestBlockEntity state at BlockPos{x=33, y=-60, z=5}, ",
            "got Block{minecraft:air}",
        )]
    );
}

#[test]
fn a_banner_block_entity_on_air_fails_in_its_constructor_cast() {
    let (failed, logs) = create_logs(BlockEntityType::BANNER, "air", BlockPos::new(15, -60, 3));
    assert!(failed);
    assert_eq!(
        normalised(&logs),
        [concat!(
            "ERROR: Failed to create block entity net.minecraft.world.level.block.entity.BlockEntityType@<hash> ",
            "for block Block{minecraft:air} at position BlockPos{x=15, y=-60, z=3} \n",
            "java.lang.ClassCastException: class net.minecraft.world.level.block.AirBlock cannot be cast to class ",
            "net.minecraft.world.level.block.AbstractBannerBlock (net.minecraft.world.level.block.AirBlock and ",
            "net.minecraft.world.level.block.AbstractBannerBlock are in unnamed module of loader java.net.URLClassLoader @0)",
        )]
    );
}

#[test]
fn no_block_entity_type_panics_on_a_block_that_is_not_its_own() {
    for ty in BlockEntityType::iter() {
        let (failed, _) = create_logs(ty, "air", BlockPos::new(0, 0, 0));
        assert!(failed, "{}", ty.name());
        for block in ty.data().valid_blocks {
            let state = block.default_state();
            assert!(
                create_block_entity_for_load(ty, BlockPos::new(0, 0, 0), state).is_ok(),
                "{} on {state}",
                ty.name()
            );
        }
    }
}
