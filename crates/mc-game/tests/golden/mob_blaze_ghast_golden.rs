//! Blazes and ghasts against vanilla. Scenarios: `tests/oracle/scenarios/mob_{blaze,ghast}_*.txt`, fixtures:
//! `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;

/// Fireball volleys, the melee swing, the hover offset and a blaze in water.
#[test]
fn blaze_fireballs() {
    check("mob_blaze_fireballs");
}

/// The fireball charge, the shot with the ghast's explosion power and the facing of the move control.
#[test]
fn ghast_fireball() {
    check("mob_ghast_fireball");
}

/// Random flight targets in the open and inside a cage.
#[test]
fn ghast_wander() {
    check("mob_ghast_wander");
}

/// A player's hit turns the fireball: the player owns it and the ghast dies of it.
#[test]
fn ghast_deflect() {
    check("mob_ghast_deflect");
}
