//! Sniffer behaviour (the brain states and digging, the seed drop, breeding and tempting, egg hatching) replayed against the
//! vanilla oracle recordings (`tests/oracle/scenarios/`, fixtures `tests/fixtures/entity_oracle/`, recorded with
//! `scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn sniffer_brain_walks_through_every_state() {
    check("mob_sniffer_states");
}

#[test]
fn sniffer_digs_up_a_seed_and_remembers_the_spot() {
    check("mob_sniffer_dig_loot");
}

#[test]
fn sniffers_breed_on_torchflower_seeds_and_lay_an_egg() {
    check("mob_sniffer_breed");
}

#[test]
fn sniffer_eggs_hatch_into_baby_sniffers() {
    check("mob_sniffer_egg_hatch");
}
