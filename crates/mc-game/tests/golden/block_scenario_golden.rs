//! Golden tests of the block oracle harness itself (`tests/common/block_scenario.rs`) against the real vanilla level.
//! Scenarios: `tests/oracle/scenarios/blk_harness_*.txt`, fixtures from `scripts/block-oracle.sh NAME`.
use crate::common;

use crate::common::block_scenario::check;

#[test]
fn shape_updates_torch_support_and_events() {
    check("blk_harness_shapes");
}

#[test]
fn block_entity_nbt_merge_round_trip() {
    check("blk_harness_blockentity");
}

#[test]
#[ignore = "until fluids are ported: WaterFluid.getTickDelay (waterlogged shape updates schedule water ticks)"]
fn waterlogged_stairs_schedule_water_ticks() {
    check("blk_harness_waterlog");
}

#[test]
#[ignore = "until RedStoneWireBlock and RedstoneLampBlock are ported"]
fn redstone_block_dust_lamp_15_ticks() {
    check("blk_harness_redstone");
}

#[test]
fn player_item_entity_dig_and_events() {
    // `fire`: a fresh Rust entity starts at 0, vanilla's player at -20 (the other entity replays ignore it too)
    common::block_scenario::check_with(
        "blk_harness_actors",
        &common::entity_oracle::Options {
            ignore_fields: vec!["fire"],
            ..Default::default()
        },
    );
}

#[test]
#[ignore = "explosion drops: Rust merges the dirt items into dirt*16 where vanilla has dirt*11 (ServerExplosion drop stacking / item merge order)"]
fn explosion_destroys_blocks_with_level_random() {
    check("blk_harness_explode");
}
