//! `net.minecraft.world.level.levelgen.structure.templatesystem`: structure templates, their placement settings and
//! the processor / rule-test system, plus the template manager and its loaders.
//!
//! Type tables (linkme, filled by each type's own file): `rule_test::RULE_TEST_TYPES`,
//! `pos_rule_test::POS_RULE_TEST_TYPES`, [`structure_processor::STRUCTURE_PROCESSOR_TYPES`],
//! `rule::blockentity::RULE_BLOCK_ENTITY_MODIFIER_TYPES`; the `worldgen/processor_list` registry is
//! [`structure_processor_list::load_processor_lists`].

pub mod block_rot_processor;
pub mod gravity_processor;
pub mod liquid_block_container;
pub mod liquid_settings;
pub mod loader;
pub mod structure_place_settings;
pub mod structure_processor;
pub mod structure_processor_list;
pub mod structure_template;
pub mod structure_template_manager;

pub use liquid_settings::LiquidSettings;
pub use structure_place_settings::StructurePlaceSettings;
pub use structure_processor::StructureProcessor;
pub use structure_processor_list::StructureProcessorList;
pub use structure_template::{StructureBlockInfo, StructureTemplate};
pub use structure_template_manager::StructureTemplateManager;
