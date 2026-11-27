//! The test block and test instance block packets and a block based game test run, replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`, verbs and dumps in `tests/oracle/GameTestVerbs.java`) vs our ticking level.

use crate::common;

#[test]
fn set_test_block_packets_change_test_blocks_of_creative_operators_only() {
    common::block_scenario::check("blk_test_block_packet");
}

#[test]
fn test_instance_block_actions_set_reset_save_and_answer_queries() {
    common::block_scenario::check("blk_test_instance_block");
}

#[test]
fn block_based_tests_started_from_instance_blocks_pass_fail_and_log() {
    common::block_scenario::check("gt_block_based_run");
}
