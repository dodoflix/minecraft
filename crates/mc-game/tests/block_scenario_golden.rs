//! Golden tests of the block oracle harness itself (`tests/common/block_scenario.rs`) against the real vanilla level.
//! Scenarios: `tests/oracle/scenarios/blk_harness_*.txt`, fixtures from `scripts/block-oracle.sh NAME`.
mod common;
use common::block_scenario::check;

#[test]
fn shape_updates_torch_support_and_events() {
    check("blk_harness_shapes");
}

#[test]
fn block_entity_nbt_merge_round_trip() {
    check("blk_harness_blockentity");
}

#[test]
#[ignore = "until BxFluids: WaterFluid.getTickDelay (waterlogged shape updates schedule water ticks)"]
fn waterlogged_stairs_schedule_water_ticks() {
    check("blk_harness_waterlog");
}

#[test]
#[ignore = "until BxRedstoneWire (RedStoneWireBlock) and BxRedstoneDevices (RedstoneLampBlock) land"]
fn redstone_block_dust_lamp_15_ticks() {
    check("blk_harness_redstone");
}
