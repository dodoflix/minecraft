//! `net.minecraft.world.level.levelgen.VerticalAnchor`.

use serde_json::Value;

use crate::data::LoadError;
use crate::densityfunction::codec::{Fields, LoadResult, parse_int_range};
use crate::noise_settings::{DIMENSION_MAX_Y, DIMENSION_MIN_Y};
use crate::world_generation_context::WorldGenerationContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VerticalAnchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
    RelativeToSeaLevel(i32),
}

impl VerticalAnchor {
    /// `VerticalAnchor.BOTTOM`.
    pub const BOTTOM: VerticalAnchor = VerticalAnchor::AboveBottom(0);
    /// `VerticalAnchor.TOP`.
    pub const TOP: VerticalAnchor = VerticalAnchor::BelowTop(0);

    pub fn sea_level() -> VerticalAnchor {
        VerticalAnchor::RelativeToSeaLevel(0)
    }

    /// `resolveY(context)`.
    pub fn resolve_y(&self, context: &WorldGenerationContext) -> i32 {
        match *self {
            VerticalAnchor::Absolute(y) => y,
            VerticalAnchor::AboveBottom(offset) => context.get_min_gen_y() + offset,
            VerticalAnchor::BelowTop(offset) => {
                context.get_gen_depth() - 1 + context.get_min_gen_y() - offset
            }
            VerticalAnchor::RelativeToSeaLevel(offset) => context.sea_level() + offset,
        }
    }

    /// `VerticalAnchor.CODEC`: `xor(absolute, xor(above_bottom, xor(below_top, relative_to_sea_level)))` over the
    /// single-field alternatives (each an int in the dimension Y range). The nesting is observable: a failing
    /// alternative never errors by itself, and `{absolute, above_bottom, below_top}` decodes as `absolute`.
    pub fn from_json(value: &Value) -> LoadResult<VerticalAnchor> {
        let fields = Fields::new(value)?;
        let range = |v: &Value| parse_int_range(v, DIMENSION_MIN_Y, DIMENSION_MAX_Y);
        let alternative =
            |name: &str, make: fn(i32) -> VerticalAnchor| fields.required(name, range).map(make);
        let xor = |left: LoadResult<VerticalAnchor>, right: LoadResult<VerticalAnchor>| match (
            left, right,
        ) {
            (Ok(_), Ok(_)) => Err(LoadError::new("Both alternatives read successfully")),
            (Ok(anchor), Err(_)) | (Err(_), Ok(anchor)) => Ok(anchor),
            (Err(e), Err(_)) => Err(e),
        };
        let inner = xor(
            alternative("below_top", VerticalAnchor::BelowTop),
            alternative("relative_to_sea_level", VerticalAnchor::RelativeToSeaLevel),
        );
        let inner = xor(
            alternative("above_bottom", VerticalAnchor::AboveBottom),
            inner,
        );
        xor(alternative("absolute", VerticalAnchor::Absolute), inner)
    }
}
