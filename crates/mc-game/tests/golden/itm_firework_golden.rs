//! `FireworkRocketItem` against vanilla: `useOn` (`tests/oracle/scenarios/itm_firework_ground.txt`, fixture regenerated with
//! `scripts/block-oracle.sh itm_firework_ground`) and `use` while gliding (`itm_firework_elytra.txt`, `scripts/entity-oracle.sh`).

use crate::common;

#[test]
fn rocket_placed_on_a_block_face_matches_vanilla() {
    common::block_scenario::check("itm_firework_ground");
}

#[test]
fn rocket_boosts_a_gliding_player_matches_vanilla() {
    common::entity_scenario::run_scenario_test("itm_firework_elytra");
}
