pub mod all_of_rule_test;

pub mod always_true_test;

pub mod any_of_rule_test;

pub mod axis_aligned_linear_pos_test;

pub mod blackstone_replace_processor;

pub mod block_age_processor;

pub mod block_ignore_processor;

pub mod block_match_test;

pub mod block_state_match_test;

pub mod capped_processor;

pub mod height_match_test;

pub mod jigsaw_replacement_processor;

pub mod lava_submerged_block_processor;

pub mod linear_pos_test;

pub mod nop_processor;

pub mod not_rule_test;

pub mod pos_always_true_test;

pub mod pos_rule_test;

pub mod processor_rule;

pub mod protected_block_processor;

pub mod random_block_match_test;

pub mod random_block_state_match_test;

pub mod rule;

pub mod rule_processor;

pub mod rule_test;

pub mod tag_match_test;

#[cfg(test)]
mod tests;
