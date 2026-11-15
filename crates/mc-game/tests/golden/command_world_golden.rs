//! The world and state commands (`/setblock /fill /clone /fillbiome /time /weather /worldborder /tick /spreadplayers /playsound` and the
//! rest) against the real vanilla level: feedback lines (`CF`), the block, biome and level state they change and the packets they send.
//! Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn block_editing_commands_match_vanilla() {
    common::block_scenario::check("cmd_world_edit");
}

#[test]
fn time_weather_difficulty_and_spawn_commands_match_vanilla() {
    common::block_scenario::check("cmd_world_time");
}

#[test]
fn world_border_commands_and_effects_match_vanilla() {
    common::block_scenario::check("cmd_worldborder_effect");
}

#[test]
fn tick_modes_match_vanilla() {
    common::block_scenario::check("cmd_tick_modes");
}

#[test]
fn sound_particle_and_post_effect_packets_match_vanilla() {
    common::block_scenario::check("cmd_fx_packets");
}

#[test]
fn waypoint_commands_and_locator_packets_match_vanilla() {
    common::block_scenario::check("cmd_waypoint");
}

#[test]
fn dialog_commands_match_vanilla() {
    common::block_scenario::check("cmd_dialog");
}

#[test]
fn spreadplayers_matches_vanilla() {
    common::block_scenario::check("cmd_spreadplayers");
}

#[test]
fn forceload_locate_random_stopwatch_help_and_list_match_vanilla() {
    common::block_scenario::check("cmd_world_misc");
}

#[test]
fn place_commands_match_vanilla() {
    common::block_scenario::check("cmd_place");
}

#[test]
fn locate_on_a_noise_level_matches_vanilla() {
    common::block_scenario::check("cmd_locate_normal");
}
