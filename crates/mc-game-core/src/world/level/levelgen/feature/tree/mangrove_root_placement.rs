//! `rootplacers.MangroveRootPlacement`.

use crate::world::level::levelgen::codec::{BlockSet, Fields, LoadResult, parse_float_range, parse_int_range};
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use serde_json::Value;

pub struct MangroveRootPlacement {
    pub can_grow_through: BlockSet,
    pub muddy_roots_in: BlockSet,
    pub muddy_roots_provider: Holder<dyn BlockStateProvider>,
    pub max_root_width: i32,
    pub max_root_length: i32,
    pub random_skew_chance: f32,
}

impl MangroveRootPlacement {
    pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<MangroveRootPlacement> {
        let fields = Fields::new(value)?;
        Ok(MangroveRootPlacement {
            can_grow_through: fields.required("can_grow_through", BlockSet::from_json)?,
            muddy_roots_in: fields.required("muddy_roots_in", BlockSet::from_json)?,
            muddy_roots_provider: fields.required("muddy_roots_provider", |v| ctx.block_state_provider(v))?,
            max_root_width: fields.required("max_root_width", |v| parse_int_range(v, 1, 12))?,
            max_root_length: fields.required("max_root_length", |v| parse_int_range(v, 1, 64))?,
            random_skew_chance: fields.required("random_skew_chance", |v| parse_float_range(v, 0.0, 1.0))?,
        })
    }
}
