//! Statistics against the vanilla oracle (ticking-level replay): every custom statistic some scenario earns, dumped as `STAT` lines
//! after each step. Scenarios: `tests/oracle/scenarios/stat_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Walking, sprinting, crouching, jumping, falling, climbing, wading, diving, swimming and gliding, moved by movement packets.
#[test]
fn moving_awards_the_distance_statistics() {
    check("stat_movement");
}

/// Riding a minecart, a happy ghast and a saddled nautilus.
#[test]
fn riding_awards_the_vehicle_statistics() {
    check("stat_vehicles");
}

/// Damage dealt to absorption hearts and to a resistance effect, damage resisted, damage a shield blocks.
#[test]
fn damage_awards_the_absorption_resistance_and_shield_statistics() {
    check("stat_combat");
}

/// Cake, flower pot, jukebox, note block, cauldrons and beds.
#[test]
fn block_use_awards_the_block_statistics() {
    check("stat_block_use");
}

/// Objectives with a statistic criterion follow the player's statistics: custom, mined, used, broken, picked up, dropped, crafted,
/// killed and killed by.
#[test]
fn scoreboard_criteria_follow_the_statistics() {
    check("stat_objectives");
}

/// The statistics a client requests, and those a leaving player ends with.
#[test]
fn requested_and_final_statistics() {
    check("stat_request_leave");
}

/// Item, block and entity statistic objectives, kill criteria by team, and the statistics of a player who left.
#[test]
fn statistic_objectives_and_kill_criteria_match_vanilla() {
    check("stat_criteria_objectives");
}

/// Play time, deaths and time since death tick by tick across death, respawn and leaving.
#[test]
fn play_time_across_death_respawn_and_leave_matches_vanilla() {
    check("stat_respawn_play_time");
}
