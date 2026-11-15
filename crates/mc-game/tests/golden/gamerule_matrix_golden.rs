//! `/gamerule` against the real vanilla level: the feedback of the query, set and invalid forms of every rule, and what each rule changes
//! in play. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn gamerule_command_feedback_matches_vanilla() {
    common::block_scenario::check("gamerule_matrix_feedback");
}

#[test]
fn rule_changes_send_their_packets() {
    common::block_scenario::check("gamerule_changed_packets");
}

#[test]
fn login_packet_carries_the_stored_flags() {
    common::block_scenario::check("gamerule_join_flags");
}

#[test]
fn block_and_fork_limits_follow_their_rules() {
    common::block_scenario::check("gamerule_command_limits");
}

#[test]
fn pvp_rule_stops_player_attacks() {
    common::block_scenario::check("gamerule_pvp");
}

#[test]
fn explosion_drop_decay_follows_the_source_rule() {
    common::block_scenario::check("gamerule_explosion_decay");
}

#[test]
fn global_sound_events_reach_far_players() {
    common::block_scenario::check("gamerule_global_sound_events");
}

#[test]
fn weather_advances_only_with_its_rule() {
    common::block_scenario::check("gamerule_weather");
}

#[test]
fn sleeping_percentage_decides_the_night_skip() {
    common::block_scenario::check("gamerule_sleep_percentage");
}

#[test]
fn movement_checks_follow_their_rules() {
    common::block_scenario::check("gamerule_movement_checks");
}

#[test]
fn snow_accumulates_up_to_its_rule() {
    common::block_scenario::check("gamerule_snow_height");
}

#[test]
fn spectators_generate_chunks_only_with_their_rule() {
    common::block_scenario::check("gamerule_spectator_chunks");
}

#[test]
fn command_block_output_reaches_operators_only_with_its_rule() {
    common::block_scenario::check("gamerule_command_block_output");
}

#[test]
fn admin_messages_and_the_console_log_follow_their_rules() {
    common::block_scenario::check("gamerule_log_admin_commands");
}

#[test]
fn respawn_radius_bounds_the_spawn_search() {
    common::block_scenario::check("gamerule_respawn_radius");
}

#[test]
fn portal_delay_rules_set_the_time_in_the_portal() {
    common::block_scenario::check("gamerule_portal_delays");
}

/// Rule -> the recorded scenarios whose output depends on it (alphabetical). A scenario of another topic counts when it sets the rule
/// (`gamerule`, `bootrule`) and a golden test replays it.
const PROOFS: &[(&str, &[&str])] = &[
    ("advance_time", &["gamerule_changed_packets"]),
    ("advance_weather", &["gamerule_weather"]),
    (
        "allow_entering_nether_using_portals",
        &["gamerule_portal_delays"],
    ),
    ("block_drops", &["blk_sl_leaves"]),
    ("block_explosion_drop_decay", &["blk_infra_explode_drops"]),
    ("command_block_output", &["gamerule_command_block_output"]),
    ("command_blocks_work", &["blk_cmdblock_chain"]),
    ("drowning_damage", &["damage_drown_freeze"]),
    ("elytra_movement_check", &["gamerule_movement_checks"]),
    ("ender_pearls_vanish_on_death", &["ent_pearl_owner_states"]),
    ("entity_drops", &["ent_minecart_drops"]),
    ("fall_damage", &["damage_sources"]),
    ("fire_damage", &["damage_sources"]),
    ("fire_spread_radius_around_player", &["blk_fluid_lava_fire"]),
    ("forgive_dead_players", &["death_basic"]),
    ("freeze_damage", &["damage_drown_freeze"]),
    ("global_sound_events", &["gamerule_global_sound_events"]),
    (
        "immediate_respawn",
        &["gamerule_changed_packets", "gamerule_join_flags"],
    ),
    ("keep_inventory", &["death_basic"]),
    ("lava_source_conversion", &["blk_fluid_gamerules"]),
    ("limited_crafting", &["mnu_table_limited_unlock"]),
    ("locator_bar", &["gamerule_changed_packets"]),
    ("log_admin_commands", &["gamerule_log_admin_commands"]),
    ("max_block_modifications", &["gamerule_command_limits"]),
    ("max_command_forks", &["gamerule_command_limits"]),
    ("max_command_sequence_length", &["blk_cmdblock_chain"]),
    ("max_entity_cramming", &["ent_push_pairs"]),
    ("max_minecart_speed", &["ent_new_max_speed"]),
    ("max_snow_accumulation_height", &["gamerule_snow_height"]),
    ("mob_drops", &["mob_creeper"]),
    ("mob_explosion_drop_decay", &["gamerule_explosion_decay"]),
    ("mob_griefing", &["blk_proj_candles"]),
    ("natural_health_regeneration", &["food_norule"]),
    ("player_movement_check", &["gamerule_movement_checks"]),
    (
        "players_nether_portal_creative_delay",
        &["gamerule_portal_delays"],
    ),
    (
        "players_nether_portal_default_delay",
        &["gamerule_portal_delays", "ll_portal_fall"],
    ),
    (
        "players_sleeping_percentage",
        &["gamerule_sleep_percentage"],
    ),
    ("projectiles_can_break_blocks", &["blk_proj_chorus"]),
    ("pvp", &["gamerule_pvp"]),
    ("raids", &["mob_raid_start"]),
    ("random_tick_speed", &["blk_proj_chorus"]),
    (
        "reduced_debug_info",
        &["gamerule_changed_packets", "gamerule_join_flags"],
    ),
    ("respawn_radius", &["gamerule_respawn_radius"]),
    ("send_command_feedback", &["cmd_perm_levels"]),
    ("show_advancement_messages", &["adv_announce_off"]),
    ("show_death_messages", &["death_basic"]),
    ("spawn_mobs", &["blk_trial_spawner_cycle"]),
    ("spawn_monsters", &["mob_spawnpass_peaceful"]),
    ("spawn_patrols", &["mob_spawner_patrol"]),
    ("spawn_phantoms", &["mob_custom_spawner_phantom"]),
    ("spawn_wandering_traders", &["mob_trader_spawner"]),
    ("spawn_wardens", &["mob_shrieker_summon"]),
    ("spawner_blocks_work", &["itm_spawn_egg"]),
    ("spectators_generate_chunks", &["gamerule_spectator_chunks"]),
    ("spread_vines", &["blk_vine_spread"]),
    ("tnt_explodes", &["blk_fbc_tnt"]),
    ("tnt_explosion_drop_decay", &["gamerule_explosion_decay"]),
    ("universal_anger", &["mob_anger_neutral"]),
    ("water_source_conversion", &["blk_fluid_water_infinite"]),
];

/// Every game rule has a recorded scenario that sets it, a fixture, and a golden test that replays it.
#[test]
fn every_rule_has_a_replayed_scenario_that_sets_it() {
    use mc_game::world::level::gamerules::AnyGameRule;
    use std::collections::BTreeSet;
    let rules: BTreeSet<String> = AnyGameRule::all().map(|rule| rule.id()).collect();
    let listed: BTreeSet<String> = PROOFS.iter().map(|(rule, _)| (*rule).to_owned()).collect();
    assert_eq!(listed.len(), PROOFS.len(), "a rule is listed twice");
    assert_eq!(
        rules.difference(&listed).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "rules without a scenario"
    );
    assert_eq!(
        listed.difference(&rules).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "listed names that are no rule"
    );
    // the golden tests of this crate and of `mc-game-core` (which replays the food scenarios)
    let tests: String = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../mc-game-core/tests/golden"),
    ]
    .iter()
    .flat_map(|dir| std::fs::read_dir(dir).expect("golden test directory"))
    .map(|entry| entry.expect("golden test entry").path())
    .filter_map(|path| {
        let text = std::fs::read_to_string(&path).ok()?;
        // this file names every scenario in the table below: only its replaying calls count
        Some(if path.ends_with("gamerule_matrix_golden.rs") {
            text.lines()
                .filter(|l| l.contains("::check("))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            text
        })
    })
    .collect();
    let read = |dir: std::path::PathBuf, name: &str| {
        let plain = dir.join(format!("{name}.txt"));
        let zipped = plain.with_extension("txt.gz");
        match std::fs::read_to_string(&plain) {
            Ok(text) => Some(text),
            Err(_) => zipped.exists().then(|| mc_testkit::read_gz(zipped)),
        }
    };
    for (rule, scenarios) in PROOFS {
        assert!(!scenarios.is_empty(), "{rule}: no scenario");
        for name in *scenarios {
            let text = read(common::entity_oracle::scenario_dir(), name)
                .unwrap_or_else(|| panic!("{rule}: scenario {name} does not exist"));
            let sets_it = text
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .any(|l| {
                    let mut words = l.split_whitespace();
                    matches!(
                        words.next(),
                        Some("gamerule" | "bootrule" | "cmd" | "cmd_as" | "cmd_inform")
                    ) && l
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|word| word == *rule)
                });
            assert!(sets_it, "{rule}: scenario {name} never sets it");
            assert!(
                read(common::entity_oracle::fixture_dir(), name).is_some(),
                "{rule}: scenario {name} has no fixture"
            );
            assert!(
                tests.contains(&format!("\"{name}\"")),
                "{rule}: no golden test replays scenario {name}"
            );
        }
    }
}
