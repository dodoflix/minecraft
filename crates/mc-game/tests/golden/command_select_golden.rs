//! Entity selectors against the real vanilla level: every option, the sort and limit rules, the selector letters, name and UUID
//! selectors, and the commands that select. Scenarios `tests/oracle/scenarios/NAME.txt` (each prints `dump select:` lines), fixtures
//! recorded with `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn sort_and_limit_match_vanilla() {
    common::block_scenario::check("cmd_select_sort_limit");
}

#[test]
fn distance_and_volume_match_vanilla() {
    common::block_scenario::check("cmd_select_distance_volume");
}

#[test]
fn scores_match_vanilla() {
    common::block_scenario::check("cmd_select_scores");
}

#[test]
fn tags_names_and_uuids_match_vanilla() {
    common::block_scenario::check("cmd_select_tags_names");
}

#[test]
fn teams_and_pet_teams_match_vanilla() {
    common::block_scenario::check("cmd_select_team");
}

#[test]
fn pet_teams_follow_owner_chains_through_horses_and_vexes() {
    common::block_scenario::check("cmd_select_team_owners");
}

#[test]
fn player_state_and_rotations_match_vanilla() {
    common::block_scenario::check("cmd_select_player_state");
}

#[test]
fn types_and_type_tags_match_vanilla() {
    common::block_scenario::check("cmd_select_types");
}

#[test]
fn predicate_option_matches_pack_predicates_like_vanilla() {
    common::block_scenario::check("cmd_select_predicate_pack");
}

#[test]
fn nbt_option_matches_vanilla() {
    common::block_scenario::check("cmd_select_nbt");
}

#[test]
fn commands_with_selectors_match_vanilla() {
    common::block_scenario::check("cmd_select_commands");
}

#[test]
fn selector_letters_states_and_parse_errors_match_vanilla() {
    common::block_scenario::check("cmd_select_basics");
}
