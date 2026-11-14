//! The ender dragon and its hitboxes against vanilla. Scenarios: `tests/oracle/scenarios/mob_dragon_*.txt`, fixtures:
//! `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Summoning takes nine entity ids, the parts of a NoAI dragon stay at the origin, only the dragon is sent to a watcher, and the parts
/// leave with their dragon.
#[test]
fn dragon_parts() {
    check("mob_dragon_parts");
}

/// A hit on a part is the dragon's hurt for that part (scaled damage, routed to the health): direct hurts, a player's sword and arrows.
#[test]
fn dragon_hurt_part() {
    check("mob_dragon_hurt_part");
}

/// Explosions hurt every part in reach on its own, and the dragon itself as its body.
#[test]
fn dragon_explosion() {
    check("mob_dragon_explosion");
}
