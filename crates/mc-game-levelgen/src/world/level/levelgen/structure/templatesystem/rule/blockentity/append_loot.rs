//! `AppendLoot` (`append_loot`).

use super::rule_block_entity_modifier::{
    RULE_BLOCK_ENTITY_MODIFIER_TYPES, RuleBlockEntityModifier,
};
use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_nbt::CompoundTag;
use std::sync::Arc;

pub struct AppendLoot {
    /// `ResourceKey<LootTable>`.
    pub loot_table: Identifier,
}

impl AppendLoot {
    pub fn decode(
        f: &Fields<'_>,
        _ctx: &DecodeCtx<'_>,
    ) -> LoadResult<Arc<dyn RuleBlockEntityModifier>> {
        Ok(Arc::new(AppendLoot {
            loot_table: f.required("loot_table", parse_identifier)?,
        }))
    }
}

impl RuleBlockEntityModifier for AppendLoot {
    fn apply(
        &self,
        random: &mut dyn RandomSource,
        existing_tag: Option<&CompoundTag>,
    ) -> Option<CompoundTag> {
        let mut result = existing_tag.map_or_else(CompoundTag::new, CompoundTag::copy);
        result.put_string("LootTable", self.loot_table.to_string());
        result.put_long("LootTableSeed", random.next_long());
        Some(result)
    }
}

#[linkme::distributed_slice(RULE_BLOCK_ENTITY_MODIFIER_TYPES)]
static TYPE: TypeEntry<dyn RuleBlockEntityModifier> = TypeEntry {
    name: "append_loot",
    decode: AppendLoot::decode,
};
