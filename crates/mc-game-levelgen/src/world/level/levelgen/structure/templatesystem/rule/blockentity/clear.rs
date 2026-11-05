//! `Clear` (`clear`).

use super::rule_block_entity_modifier::{RULE_BLOCK_ENTITY_MODIFIER_TYPES, RuleBlockEntityModifier};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_core::random::RandomSource;
use mc_nbt::CompoundTag;
use std::sync::Arc;

pub struct Clear;

impl RuleBlockEntityModifier for Clear {
    fn apply(&self, _random: &mut dyn RandomSource, _existing_tag: Option<&CompoundTag>) -> Option<CompoundTag> {
        Some(CompoundTag::new())
    }
}

#[linkme::distributed_slice(RULE_BLOCK_ENTITY_MODIFIER_TYPES)]
static TYPE: TypeEntry<dyn RuleBlockEntityModifier> = TypeEntry { name: "clear", decode: |_, _| Ok(Arc::new(Clear)) };
