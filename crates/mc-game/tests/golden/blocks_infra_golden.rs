//! Goldens of the `BlockBehaviour` surface (`getDrops` hooks, destroy particles, `BonemealableBlock` server dispatch,
//! container removal): scenarios `tests/oracle/scenarios/blk_infra_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_infra_*.txt`).

use crate::common;

#[test]
fn pot_drops_and_destroy_particles() {
    common::block_scenario::check("blk_infra_pot_drops");
}

#[test]
fn bonemeal_server_dispatch() {
    common::block_scenario::check("blk_infra_bonemeal_server");
}

#[test]
fn container_contents_drop_when_the_block_goes() {
    common::block_scenario::check("blk_infra_break_drops_container");
}
