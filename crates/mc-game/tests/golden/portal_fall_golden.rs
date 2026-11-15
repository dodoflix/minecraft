//! A falling entity that moves through a portal block resets its fall distance only as `ClipContext.Block.FALLDAMAGE_RESETTING`
//! says (`tests/oracle/scenarios/ll_portal_fall.txt`; fixture regenerated with `scripts/block-oracle.sh ll_portal_fall`).

use crate::common;

#[test]
fn portal_blocks_reset_the_fall_distance_of_players_only() {
    common::block_scenario::check("ll_portal_fall");
}
