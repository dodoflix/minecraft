//! Rails vs the real vanilla level (`tests/oracle/scenarios/blk_rails_*.txt`, fixtures from `scripts/block-oracle.sh NAME`).
mod common;
use common::block_scenario::check;

#[test]
fn rail_network_placement_orders_and_shapes() {
    check("blk_rails_network");
}

#[test]
fn powered_and_activator_rail_propagation() {
    check("blk_rails_powered");
}

#[test]
#[ignore = "until the block oracle replay supports spawn/spawn_item/kill (minecart entities)"]
fn detector_rail_minecart_detection() {
    check("blk_rails_detector");
}
