//! The wither against vanilla. Scenarios: `tests/oracle/scenarios/mob_wither_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// The wither summoned from its pattern charges up invulnerable with a growing boss bar and ends with a blast.
#[test]
fn wither_spawn_invulnerable_charge() {
    check("mob_wither_spawn_invul");
}

/// The heads pick their own targets, turn towards them, shoot, and idle into blue skulls when nobody is left.
#[test]
fn wither_heads_target_and_idle() {
    check("mob_wither_heads");
}

/// Skulls hit with damage, the wither effect of the difficulty and an explosion; a kill heals the wither.
#[test]
fn wither_skulls_hit_and_explode() {
    check("mob_wither_skulls");
}

/// At half health arrows and wind charges bounce off; a hit starts the block breaking countdown.
#[test]
fn wither_half_health_armour_and_block_breaking() {
    check("mob_wither_half_health");
}

/// The boss bar is added, updated and removed with the players tracking the wither.
#[test]
fn wither_boss_bar_events() {
    check("mob_wither_bossbar");
}
