//! The allay against vanilla: items handed over and fetched, the liked player and note block, jukebox dancing, duplication and the
//! dispenser (`tests/oracle/scenarios/mob_allay_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::blocks_dispenser_spine_golden::check;

#[test]
fn pickup_and_give() {
    check("mob_allay_pickup_give");
}

#[test]
fn noteblock_and_jukebox() {
    check("mob_allay_noteblock_dance");
}

#[test]
fn duplicate() {
    check("mob_allay_duplicate");
}

#[test]
fn liked_player() {
    check("mob_allay_liked_player");
}

#[test]
fn dispenser_equip() {
    check("mob_allay_dispenser_equip");
}
