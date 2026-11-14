//! Endermen and phantoms against vanilla, and neutral mobs loaded with a saved anger target. Scenarios:
//! `tests/oracle/scenarios/mob_{enderman,phantom,anger}_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Neutral mobs loaded from saved data (spawn merge, summon) that names the player they are angry at.
#[test]
fn anger_load() {
    check("mob_anger_load");
}

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
