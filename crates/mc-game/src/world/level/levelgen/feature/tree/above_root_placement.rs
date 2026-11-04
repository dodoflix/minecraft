//! `rootplacers.AboveRootPlacement`.

use crate::world::level::levelgen::codec::{Fields, LoadResult, parse_float_range};
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use serde_json::Value;

pub struct AboveRootPlacement {
    pub above_root_provider: Holder<dyn BlockStateProvider>,
    pub above_root_placement_chance: f32,
}

impl AboveRootPlacement {
    pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<AboveRootPlacement> {
        let fields = Fields::new(value)?;
        Ok(AboveRootPlacement {
            above_root_provider: fields.required("above_root_provider", |v| ctx.block_state_provider(v))?,
            above_root_placement_chance: fields.required("above_root_placement_chance", |v| parse_float_range(v, 0.0, 1.0))?,
        })
    }
}
