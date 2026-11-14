//! Mob effects with consequences for mobs against the vanilla oracle (scenarios in `oracle/scenarios`): oozing, infested,
//! wind charged and weaving.
use crate::common::block_scenario;

#[test]
fn infested_mob_hurt_spawns_silverfish() {
    block_scenario::check("mob_effect_infested");
}

#[test]
fn oozing_mob_death_spawns_slimes_up_to_the_cramming_limit() {
    block_scenario::check("mob_effect_oozing");
}

#[test]
fn weaving_mob_death_spreads_cobwebs() {
    block_scenario::check("mob_effect_weaving");
}

#[test]
fn wind_charged_mob_death_bursts_into_a_gust() {
    block_scenario::check("mob_effect_wind_charged");
}
