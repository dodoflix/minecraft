//! Text component resolution (`ComponentUtils.resolve`) through the commands that send text: `/tellraw` (every content type, the depth
//! limit), `/title`, `/bossbar`, `/say`, `/me`, `/msg`. Scenarios `tests/oracle/scenarios/cmd_text_*.txt`, fixtures recorded with
//! `scripts/block-oracle.sh NAME`; the packets (`PKT`) and the feedback (`CF`) are compared.

use crate::common;

#[test]
fn tellraw_contents_match_vanilla() {
    common::block_scenario::check("cmd_text_contents");
}

#[test]
fn tellraw_nbt_contents_match_vanilla() {
    common::block_scenario::check("cmd_text_nbt");
}

#[test]
fn resolution_depth_limit_matches_vanilla() {
    common::block_scenario::check("cmd_text_depth");
}

#[test]
fn resolved_component_limit_matches_vanilla() {
    common::block_scenario::check("cmd_text_limit");
}

#[test]
fn title_bossbar_and_chat_commands_match_vanilla() {
    common::block_scenario::check("cmd_text_outputs");
}
