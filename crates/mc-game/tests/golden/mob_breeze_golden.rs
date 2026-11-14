//! The breeze against the vanilla oracle (scenarios in `oracle/scenarios`): the fight (inhale, shoot, long jump, slide), the
//! breezes that cannot jump, the deflection of projectiles and the trial spawner spawning breezes.
use crate::common::block_scenario;

#[test]
fn breeze_shoots_jumps_and_slides() {
    block_scenario::check("mob_breeze_fight");
}

#[test]
fn breeze_shoots_when_stuck_and_fights_back_when_hurt() {
    block_scenario::check("mob_breeze_stuck");
}

#[test]
fn breeze_deflects_projectiles() {
    block_scenario::check("mob_breeze_deflect");
}

#[test]
fn trial_spawner_spawns_breezes() {
    block_scenario::check("mob_breeze_trial_spawner");
}
