//! Command blocks and command block minecarts running real commands against the real vanilla level: the last output with its time
//! stamp (printed as `[HH:MM:SS]` by both sides), success counts and comparators, conditional chains, repeating blocks, the
//! rules that gate them and the editor packets. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with
//! `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn command_blocks_run_real_commands_like_vanilla() {
    common::block_scenario::check("cmd_block_real");
}

#[test]
fn command_block_minecarts_run_real_commands_like_vanilla() {
    common::block_scenario::check("cmd_block_minecart");
}

#[test]
fn clock_stamps_are_normalised_in_text_and_hex_dumps() {
    use common::command_verbs::normalize_clock_stamps as normalize;
    assert_eq!(
        normalize("LastOutput:'[12:34:56] out [1:2:3] [ab:cd:ef]'"),
        "LastOutput:'[HH:MM:SS] out [1:2:3] [ab:cd:ef]'"
    );
    assert_eq!(
        normalize("14:5:0b5b31323a33343a35365d20"),
        "14:5:0b5b48483a4d4d3a53535d20"
    );
}
