//! `Passthrough` (`passthrough`).

use super::rule_block_entity_modifier::{RULE_BLOCK_ENTITY_MODIFIER_TYPES, RuleBlockEntityModifier};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_core::random::RandomSource;
use mc_nbt::CompoundTag;
use std::sync::Arc;

pub struct Passthrough;

impl RuleBlockEntityModifier for Passthrough {
    fn apply(&self, _random: &mut dyn RandomSource, existing_tag: Option<&CompoundTag>) -> Option<CompoundTag> {
        existing_tag.cloned()
    }
}

#[linkme::distributed_slice(RULE_BLOCK_ENTITY_MODIFIER_TYPES)]
static TYPE: TypeEntry<dyn RuleBlockEntityModifier> = TypeEntry { name: "passthrough", decode: |_, _| Ok(Arc::new(Passthrough)) };
