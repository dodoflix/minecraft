//! `ReplaceBlockFeature` (`replace_single_block`) and `BlockReplacement`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_block_state, parse_list};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::templatesystem::rule_test::{self, RuleTest};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

/// `BlockReplacement`.
pub struct BlockReplacement {
    pub target: Arc<dyn RuleTest>,
    pub state: BlockState,
}

fn decode_replacement(value: &serde_json::Value, ctx: &DecodeCtx<'_>) -> LoadResult<BlockReplacement> {
    let fields = Fields::new(value)?;
    Ok(BlockReplacement {
        target: fields.required("target", |v| rule_test::decode(v, ctx))?,
        state: fields.required("state", parse_block_state)?,
    })
}

pub struct ReplaceBlockFeature {
    pub replacements: Vec<BlockReplacement>,
}

impl ReplaceBlockFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(ReplaceBlockFeature { replacements: fields.required("targets", |v| parse_list(v, |r| decode_replacement(r, ctx)))? }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static REPLACE_SINGLE_BLOCK: TypeEntry<dyn Feature> = TypeEntry { name: "replace_single_block", decode: ReplaceBlockFeature::decode };

impl Feature for ReplaceBlockFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        for replacement in &self.replacements {
            if replacement.target.test(level.get_block_state(origin), origin, random) {
                level.set_block_flags(origin, replacement.state, 2);
                break;
            }
        }
        true
    }
}
