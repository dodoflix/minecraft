//! Entity interactions at a click position (`tests/oracle/scenarios/ent_interact_at.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn armor_stand_slots_follow_the_click_height() {
    common::block_scenario::check("ent_interact_at");
}
