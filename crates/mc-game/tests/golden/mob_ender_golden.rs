//! Endermen and phantoms against vanilla. Scenarios: `tests/oracle/scenarios/mob_{enderman,phantom}_*.txt`, fixtures:
//! `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// The stare that angers an enderman, the carved pumpkin and the averted gaze.
#[test]
fn enderman_stare() {
    check("mob_enderman_stare");
}

/// Taking, carrying, placing and dropping blocks, with and without mobGriefing.
#[test]
fn enderman_block() {
    check("mob_enderman_block");
}

/// Projectile and potion dodges, water and rain damage, the daylight teleport.
#[test]
fn enderman_teleport() {
    check("mob_enderman_teleport");
}

/// Circling the anchor, the target scan, the swoop and its bite, the saved anchor and size, burning at dawn.
#[test]
fn phantom_circle_swoop() {
    check("mob_phantom_circle_swoop");
}
