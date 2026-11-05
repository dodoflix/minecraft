//! `RuleBasedStateProvider` (`rule_based`).

use super::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use crate::world::level::levelgen::Holder;
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{Fields, TypeEntry, parse_list};
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

/// `RuleBasedStateProvider.Rule`.
pub struct Rule {
    pub if_true: Arc<dyn BlockPredicate>,
    pub then: Holder<dyn BlockStateProvider>,
}

pub struct RuleBasedStateProvider {
    pub fallback: Option<Holder<dyn BlockStateProvider>>,
    pub rules: Vec<Rule>,
}

impl BlockStateProvider for RuleBasedStateProvider {
    fn get_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        self.get_optional_state(level, random, pos).unwrap_or_else(|| level.get_block_state(pos))
    }

    fn get_optional_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> Option<BlockState> {
        for rule in &self.rules {
            if rule.if_true.test(level, pos)
                && let Some(state) = rule.then.value().get_optional_state(level, random, pos)
            {
                return Some(state);
            }
        }
        self.fallback.as_ref().and_then(|fallback| fallback.value().get_optional_state(level, random, pos))
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static RULE_BASED: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "rule_based",
    decode: |fields, ctx| {
        let fallback = fields.optional("fallback", |v| ctx.block_state_provider(v))?;
        let rules = fields.required("rules", |v| {
            parse_list(v, |rule| {
                let rule = Fields::new(rule)?;
                let if_true = rule.required("if_true", |v| blockpredicates::decode(v, ctx))?;
                let then = rule.required("then", |v| ctx.block_state_provider(v))?;
                Ok(Rule { if_true, then })
            })
        })?;
        Ok(Arc::new(RuleBasedStateProvider { fallback, rules }))
    },
};
