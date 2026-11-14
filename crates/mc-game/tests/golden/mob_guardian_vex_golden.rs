//! Guardians, elder guardians and vexes against vanilla. Scenarios: `tests/oracle/scenarios/mob_{guardian,elder_guardian,vex}_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;

/// The beam charge and damage on a player in a pool, swimming, and the thorns of a guardian that is not moving.
#[test]
fn guardian_beam() {
    check("mob_guardian_beam");
}

/// Guardians on land flop, a guardian in a shallow puddle, an elder guardian on land.
#[test]
fn guardian_flop() {
    check("mob_guardian_flop");
}

/// The curse of an elder guardian: survival players in range only, the appearance event, a player with a weaker effect.
#[test]
fn elder_guardian_effect() {
    check("mob_elder_guardian_effect");
}

/// A vex hunting a player behind a wall, and one idling around its bound position.
#[test]
fn vex_idle_hunt() {
    check("mob_vex_idle_hunt");
}

/// A vex copies its owner's target and starves after its life ticks.
#[test]
fn vex_owner_life() {
    check("mob_vex_owner_life");
}
