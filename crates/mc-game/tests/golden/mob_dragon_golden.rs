//! The ender dragon, its hitboxes and its phases against vanilla. Scenarios: `tests/oracle/scenarios/mob_dragon_*.txt`, fixtures:
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

/// A fresh dragon hovers around where it spawned; hits on its head that add up to a quarter of its health make it take off and fly
/// the first path of the island.
#[test]
fn dragon_hover_and_takeoff() {
    check("mob_dragon_hover");
}

/// A hurt dragon heals from the nearest end crystal, a destroyed crystal hurts its head, and with no crystal in reach it heals nothing.
#[test]
fn dragon_crystal_heal() {
    check("mob_dragon_crystal_heal");
}

/// A holding dragon turns on the player who destroyed a crystal, flies to it and spits a dragon fireball.
#[test]
fn dragon_strafe_fireball() {
    check("mob_dragon_strafe_fireball");
}

/// A landing dragon sits on the podium, scans, roars and breathes a cloud; its wings and head hurt what they touch.
#[test]
fn dragon_land_breath() {
    check("mob_dragon_land_breath");
}

/// A killing blow sends the dragon to the podium in the dying phase; its death animation rains experience and removes it.
#[test]
fn dragon_death() {
    check("mob_dragon_death");
}

/// A sitting dragon that finds nobody close takes off and charges a player in sight, then holds its pattern again.
#[test]
fn dragon_charge() {
    check("mob_dragon_charge");
}
