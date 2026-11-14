//! `Mob.checkDespawn` against vanilla: the instant range per mob category, the no-action random despawn, the peaceful rule and
//! the persistence flag (name tag, lead, passenger, picked-up item, class overrides).
//! Scenarios: `tests/oracle/scenarios/mob_despawn_*.txt` and `mob_persistence.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn class_overrides() {
    check("mob_despawn_class");
}

#[test]
fn far() {
    check("mob_despawn_far");
}

#[test]
fn noaction() {
    check("mob_despawn_noaction");
}

#[test]
fn peaceful() {
    check("mob_despawn_peaceful");
}

#[test]
fn persistence() {
    check("mob_persistence");
}

#[test]
fn patrol() {
    check("mob_despawn_patrol");
}
