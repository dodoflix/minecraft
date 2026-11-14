//! The copper golem against vanilla: weathering, waxing, scraping and lightning, the transport between chests and the
//! statue it turns into (`tests/oracle/scenarios/mob_copper_golem_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn oxidize() {
    common::block_scenario::check("mob_copper_golem_oxidize");
}

#[test]
fn transport() {
    common::block_scenario::check("mob_copper_golem_transport");
}

#[test]
fn dispenser_shears() {
    common::block_scenario::check("blk_disp_shears_copper_golem");
}

#[test]
fn statue() {
    common::block_scenario::check("mob_copper_golem_statue");
}

#[test]
fn transport_queue() {
    common::block_scenario::check("mob_copper_golem_transport_queue");
}
