//! Every block of a Java class that implements `SimpleWaterloggedBlock` is a liquid container and a bucket pickup.
use mc_data::block::Block;
use mc_game::world::level::block::behaviour;

/// Classes of `net.minecraft.world.level.block` that implement `SimpleWaterloggedBlock` directly.
const SIMPLE_WATERLOGGED: [&str; 39] = [
    "AmethystClusterBlock", "BarrierBlock", "BaseCoralPlantTypeBlock", "BaseRailBlock", "BigDripleafBlock", "BigDripleafStemBlock",
    "CampfireBlock", "CandleBlock", "ChainBlock", "ChestBlock", "ConduitBlock", "CopperGolemStatueBlock", "CrossCollisionBlock",
    "DecoratedPotBlock", "DriedGhastBlock", "EnderChestBlock", "HangingRootsBlock", "HeavyCoreBlock", "LadderBlock", "LanternBlock",
    "LeavesBlock", "LightBlock", "LightningRodBlock", "MangrovePropaguleBlock", "MangroveRootsBlock", "MultifaceBlock",
    "ScaffoldingBlock", "SculkSensorBlock", "SculkShriekerBlock", "SeaPickleBlock", "ShelfBlock", "SignBlock", "SlabBlock",
    "SmallDripleafBlock", "SpeleothemBlock", "StairBlock", "TrapDoorBlock", "WallBlock", "WaterloggedTransparentBlock",
];

#[test]
fn simple_waterlogged_blocks_are_containers() {
    let missing: Vec<&str> = Block::iter()
        .filter(|b| b.data().classes.iter().any(|c| SIMPLE_WATERLOGGED.contains(c)))
        .filter(|b| !(behaviour(*b).is_liquid_block_container() && behaviour(*b).is_bucket_pickup()))
        .map(|b| b.data().name)
        .collect();
    assert!(missing.is_empty(), "not wired to SimpleWaterloggedBlock: {missing:?}");
}
