//! `net.minecraft.world.level.levelgen.structure.templatesystem`: structure templates, their placement settings and
//! the processor / rule-test system, plus the template manager and its loaders.
//!
//! Type tables (linkme, filled by each type's own file): [`rule_test::RULE_TEST_TYPES`],
//! [`pos_rule_test::POS_RULE_TEST_TYPES`], [`structure_processor::STRUCTURE_PROCESSOR_TYPES`],
//! [`rule::blockentity::RULE_BLOCK_ENTITY_MODIFIER_TYPES`]; the `worldgen/processor_list` registry is
//! [`structure_processor_list::load_processor_lists`].

pub mod all_of_rule_test;
pub mod always_true_test;
pub mod any_of_rule_test;
pub mod axis_aligned_linear_pos_test;
pub mod blackstone_replace_processor;
pub mod block_age_processor;
pub mod block_ignore_processor;
pub mod block_match_test;
pub mod block_rot_processor;
pub mod block_state_match_test;
pub mod capped_processor;
pub mod gravity_processor;
pub mod height_match_test;
pub mod jigsaw_replacement_processor;
pub mod lava_submerged_block_processor;
pub mod linear_pos_test;
pub mod liquid_block_container;
pub mod liquid_settings;
pub mod loader;
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
pub mod structure_place_settings;
pub mod structure_processor;
pub mod structure_processor_list;
pub mod structure_template;
pub mod structure_template_manager;
pub mod tag_match_test;

#[cfg(test)]
mod tests;

pub use liquid_settings::LiquidSettings;
pub use structure_place_settings::StructurePlaceSettings;
pub use structure_processor::StructureProcessor;
pub use structure_processor_list::StructureProcessorList;
pub use structure_template::{StructureBlockInfo, StructureTemplate};
pub use structure_template_manager::StructureTemplateManager;
