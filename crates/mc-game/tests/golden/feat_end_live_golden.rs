//! The End's features placed with `/place feature` on a live End level, replayed against the real vanilla level
//! (`scripts/block-oracle.sh NAME`): blocks, block entities, crystals, drops and the level events of each placement.

use crate::common;

#[test]
fn end_gateway_placed_on_a_live_level() {
    common::block_scenario::check("feat_end_gateway_live");
}

#[test]
fn end_island_placed_on_a_live_level() {
    common::block_scenario::check("feat_end_island_live");
}

#[test]
fn end_podium_placed_on_a_live_level() {
    common::block_scenario::check("feat_end_podium_live");
}

#[test]
fn end_spike_placed_on_a_live_level() {
    common::block_scenario::check("feat_end_spike_live");
}
