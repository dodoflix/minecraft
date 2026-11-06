//! `GeodeBlockSettings`.

use mc_data::block::BlockState;
use mc_game_core::world::level::levelgen::Holder;
use mc_game_core::world::level::levelgen::codec::{
    BlockSet, Fields, LoadError, LoadResult, parse_block_state, parse_list,
};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use serde_json::Value;

pub struct GeodeBlockSettings {
    pub filling_provider: Holder<dyn BlockStateProvider>,
    pub inner_layer_provider: Holder<dyn BlockStateProvider>,
    pub alternate_inner_layer_provider: Holder<dyn BlockStateProvider>,
    pub middle_layer_provider: Holder<dyn BlockStateProvider>,
    pub outer_layer_provider: Holder<dyn BlockStateProvider>,
    /// Never empty.
    pub inner_placements: Vec<BlockState>,
    pub cannot_replace: BlockSet,
    pub invalid_blocks: BlockSet,
}

impl GeodeBlockSettings {
    pub fn from_json(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<GeodeBlockSettings> {
        let fields = Fields::new(value)?;
        Ok(GeodeBlockSettings {
            filling_provider: fields
                .required("filling_provider", |v| ctx.block_state_provider(v))?,
            inner_layer_provider: fields
                .required("inner_layer_provider", |v| ctx.block_state_provider(v))?,
            alternate_inner_layer_provider: fields
                .required("alternate_inner_layer_provider", |v| {
                    ctx.block_state_provider(v)
                })?,
            middle_layer_provider: fields
                .required("middle_layer_provider", |v| ctx.block_state_provider(v))?,
            outer_layer_provider: fields
                .required("outer_layer_provider", |v| ctx.block_state_provider(v))?,
            inner_placements: fields.required("inner_placements", |v| {
                let list = parse_list(v, parse_block_state)?;
                if list.is_empty() {
                    Err(LoadError::new("List must have contents"))
                } else {
                    Ok(list)
                }
            })?,
            cannot_replace: fields.required("cannot_replace", BlockSet::from_json)?,
            invalid_blocks: fields.required("invalid_blocks", BlockSet::from_json)?,
        })
    }
}
