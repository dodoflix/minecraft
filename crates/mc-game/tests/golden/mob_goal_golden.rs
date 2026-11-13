//! Goal contracts against vanilla: strolling and looking, panic, avoiding another entity, fleeing the sun, the target goals (alerting
//! the pack, the owner's targets, defending a village, the unseen-target memory) and a mob steering the mob it rides.
//! Scenarios: `tests/oracle/scenarios/mob_{goal,target,jockey}_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn flee_sun() {
    check("mob_goal_flee_sun");
}

#[test]
fn panic_avoid() {
    check("mob_goal_panic_avoid");
}

#[test]
fn stroll_look() {
    check("mob_goal_stroll_look");
}

#[test]
fn jockey_path() {
    check("mob_jockey_path");
}

#[test]
fn target_defend_village() {
    check("mob_target_defend_village");
}

#[test]
fn target_hurtby_alert() {
    check("mob_target_hurtby_alert");
}

#[test]
fn target_owner() {
    check("mob_target_owner");
}

#[test]
fn target_unseen_memory() {
    check("mob_target_unseen_memory");
}
