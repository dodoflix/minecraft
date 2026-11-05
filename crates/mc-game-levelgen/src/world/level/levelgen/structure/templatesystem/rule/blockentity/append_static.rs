//! `AppendStatic` (`append_static`).

use super::rule_block_entity_modifier::{RULE_BLOCK_ENTITY_MODIFIER_TYPES, RuleBlockEntityModifier};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::random::RandomSource;
use mc_nbt::CompoundTag;
use std::sync::Arc;

pub struct AppendStatic {
    pub tag: CompoundTag,
}

/// `CompoundTag.CODEC` over a JSON value (JSON numbers narrow like `JsonOps.convertTo`).
pub fn parse_compound_tag(value: &serde_json::Value) -> LoadResult<CompoundTag> {
    mc_dfu::parse_json(&mc_protocol::components::common::compound_tag_codec(), &value.to_string()).into_result().map_err(LoadError::new)
}

impl AppendStatic {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleBlockEntityModifier>> {
        Ok(Arc::new(AppendStatic { tag: f.required("data", parse_compound_tag)? }))
    }
}

impl RuleBlockEntityModifier for AppendStatic {
    fn apply(&self, _random: &mut dyn RandomSource, existing_tag: Option<&CompoundTag>) -> Option<CompoundTag> {
        Some(match existing_tag {
            None => self.tag.copy(),
            // `existingTag.merge(tag)` mutates and returns the existing tag
            Some(existing) => {
                let mut merged = existing.clone();
                merged.merge(&self.tag);
                merged
            }
        })
    }
}

#[linkme::distributed_slice(RULE_BLOCK_ENTITY_MODIFIER_TYPES)]
static TYPE: TypeEntry<dyn RuleBlockEntityModifier> = TypeEntry { name: "append_static", decode: AppendStatic::decode };
