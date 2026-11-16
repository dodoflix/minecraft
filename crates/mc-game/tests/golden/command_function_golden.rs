//! Functions, macros, `return`, `schedule`, the load and tick tags, `reload` and `execute` against the real vanilla level running a data
//! pack (`tests/oracle/datapacks/fnpack`): the feedback, failure and result lines, the storage and scoreboard state the functions leave
//! behind, and the errors vanilla logs while a library loads. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with
//! `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn function_library_logs_tags_and_reload_match_vanilla() {
    common::block_scenario::check("cmd_function_library");
}

#[test]
fn function_command_forms_tags_and_results_match_vanilla() {
    common::block_scenario::check("cmd_function_run");
}

#[test]
fn macro_functions_arguments_and_errors_match_vanilla() {
    common::block_scenario::check("cmd_function_macros");
}

#[test]
fn return_values_and_forks_match_vanilla() {
    common::block_scenario::check("cmd_function_return");
}

#[test]
fn schedule_append_replace_clear_and_order_match_vanilla() {
    common::block_scenario::check("cmd_function_schedule");
}

#[test]
fn execute_conditions_match_vanilla() {
    common::block_scenario::check("cmd_execute_conditions");
}

#[test]
fn execute_store_targets_match_vanilla() {
    common::block_scenario::check("cmd_execute_store");
}

#[test]
fn execute_context_modifiers_match_vanilla() {
    common::block_scenario::check("cmd_execute_context");
}

#[test]
fn chain_and_fork_limits_match_vanilla() {
    common::block_scenario::check("cmd_execute_limits");
}

#[test]
fn reload_of_an_edited_pack_matches_vanilla() {
    common::block_scenario::check("cmd_function_reload");
}

#[test]
fn functions_interplay_with_execute_store_schedule_and_reload_match_vanilla() {
    common::block_scenario::check("cmd_function_interplay");
}
