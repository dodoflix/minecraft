//! Camel and camel husk behaviour (sit and stand, dash, two riders, the inventory screen, breeding and tempting, desert spawning,
//! the husk jockey of a natural husk)
//! replayed against the vanilla oracle recordings (`tests/oracle/scenarios/`, fixtures `tests/fixtures/entity_oracle/`, recorded with
//! `scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn camel_sits_down_and_stands_up_like_vanilla() {
    check("mob_camel_sit_stand");
}

#[test]
fn camels_breed_on_cactus_and_follow_it() {
    check("mob_camel_breed_tempt");
}

#[test]
fn camel_dash_follows_the_riding_jump_command() {
    check("mob_camel_dash");
}

#[test]
fn camel_carries_two_riders() {
    check("mob_camel_two_riders");
}

#[test]
fn camel_inventory_screen_matches_vanilla() {
    check("mnu_camel_inventory");
}

#[test]
fn husk_jockey_rides_a_camel_husk() {
    check("mob_camel_husk_jockey");
}

#[test]
fn camels_spawn_on_sand_in_deserts() {
    check("mob_camel_spawn_desert");
}
