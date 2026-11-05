//! `BlockReplacement`: one `{target: RuleTest, state: BlockState}` entry of ore / replace-block features.

use crate::world::level::levelgen::codec::{Fields, LoadResult, parse_block_state};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::templatesystem::rule_test::{self, RuleTest};
use mc_data::block::BlockState;
use serde_json::Value;
use std::sync::Arc;

pub struct BlockReplacement {
    pub target: Arc<dyn RuleTest>,
    pub state: BlockState,
}

impl BlockReplacement {
    /// `BlockReplacement.CODEC`.
    pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<BlockReplacement> {
        let fields = Fields::new(value)?;
        Ok(BlockReplacement {
            target: fields.required("target", |v| rule_test::decode(v, ctx))?,
            state: fields.required("state", parse_block_state)?,
        })
    }
}
