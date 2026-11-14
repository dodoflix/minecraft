//! Raids against the vanilla oracle (scenarios in `oracle/scenarios`): the omen effects that start a raid, waves to victory, a
//! lost village, the raid boss bar and the captain's ominous banner.
use crate::common::block_scenario;

#[test]
fn captain_of_a_wave_drops_the_ominous_banner_and_another_raider_wears_it() {
    block_scenario::check("mob_raid_banner_leader");
}

#[test]
fn patrol_captain_drops_the_banner_and_an_ominous_bottle() {
    block_scenario::check("mob_raid_captain_drops");
}

#[test]
fn raid_boss_bar_follows_the_countdown_and_the_raiders() {
    block_scenario::check("mob_raid_bossbar");
}

#[test]
fn omen_effects_send_their_effect_packets_and_particle_data() {
    block_scenario::check("mob_raid_omen_packets");
}

#[test]
fn raid_on_hard_runs_every_wave_to_victory() {
    block_scenario::check("mob_raid_full");
}

#[test]
fn raid_saved_and_loaded_mid_wave_takes_its_raiders_back() {
    block_scenario::check("mob_raid_reload");
}

#[test]
fn raid_is_lost_when_the_village_is_gone() {
    block_scenario::check("mob_raid_loss");
}

#[test]
fn bad_omen_in_a_village_becomes_raid_omen_and_starts_a_raid() {
    block_scenario::check("mob_raid_omen_effect");
}

#[test]
fn villagers_hide_in_a_raid_and_the_bell_makes_raiders_glow() {
    block_scenario::check("mob_villager_raid_hide");
}
