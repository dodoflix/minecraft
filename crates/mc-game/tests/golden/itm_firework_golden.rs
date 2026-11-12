//! `FireworkRocketItem.useOn` against vanilla (`tests/oracle/scenarios/itm_firework_ground.txt`; fixture regenerated with
//! `scripts/block-oracle.sh itm_firework_ground`).

use crate::common;

#[test]
fn rocket_placed_on_a_block_face_matches_vanilla() {
    common::block_scenario::check("itm_firework_ground");
}
