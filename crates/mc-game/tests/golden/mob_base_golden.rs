//! Base-class mob behaviour against vanilla: persistent anger, taming and sitting, growing up, the leash tick and the state a
//! conversion carries over (`tests/oracle/scenarios/mob_*.txt`; the fixtures come from `scripts/entity-oracle.sh` and
//! `scripts/block-oracle.sh`).

use crate::blocks_dispenser_spine_golden::check;
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn anger_neutral() {
    run_scenario_test("mob_anger_neutral");
}

#[test]
fn tame_sit_follow() {
    run_scenario_test("mob_tame_sit_follow");
}

/// A tamed wolf whose owner is in another level: it sits, follows the owner's coordinates in the air, teleports around the owner's
/// block position and sends the death message to the owner.
#[test]
fn pet_owner_in_another_level() {
    check("mob_pet_owner_other_dimension");
}

#[test]
fn age_growth() {
    check("mob_age_growth");
}

#[test]
fn leash_tick() {
    run_scenario_test("mob_leash_tick");
}

#[test]
fn conversion_common() {
    check("mob_conversion_common");
}

/// A baby grows up squeezed under a ceiling, beside a wall and in a corner: `Entity.refreshDimensions` pushes it out of the blocks.
#[test]
fn baby_grows_against_blocks() {
    check("mob_baby_grow_wall");
}

/// Scoreboard team collision rules on pushing (`never`, `pushOwnTeam`, two teams, a tamed wolf taking its owner's team, a player in the team).
#[test]
fn team_push() {
    check("mob_team_push");
}

/// `tame` and `leash` take a player who is in another level than the mob.
#[test]
fn tame_and_leash_reach_a_player_in_another_level() {
    check("mob_tame_remote_owner");
}

/// `love` takes a player who is in another level than the animal.
#[test]
fn love_reaches_a_player_in_another_level() {
    check("mob_love_remote_player");
}
