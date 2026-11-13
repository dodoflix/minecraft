//! Goldens of the mob controls' turning (`MoveControl`, `LookControl`): yaw, head and body rotation and the random of a pig, a
//! cow and a zombie walking to alternating targets, against the vanilla oracle (`tests/oracle/scenarios/mob_ctrl_*.txt`).
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn mobs_turn_toward_alternating_targets() {
    run_scenario_test("mob_ctrl_turn");
}
