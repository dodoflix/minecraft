//! `templatesystem.rule.blockentity`: how a processor rule rewrites the block entity data of the block it replaces.

pub mod append_loot;
pub mod append_static;
pub mod clear;
pub mod passthrough;
pub mod rule_block_entity_modifier;

pub use rule_block_entity_modifier::{RULE_BLOCK_ENTITY_MODIFIER_TYPES, RuleBlockEntityModifier};
