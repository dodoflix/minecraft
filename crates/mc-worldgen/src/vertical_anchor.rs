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

    /// `VerticalAnchor.CODEC`: an object with exactly one of `absolute`, `above_bottom`, `below_top`,
    /// `relative_to_sea_level` (each an int in the dimension Y range).
    pub fn from_json(value: &Value) -> LoadResult<VerticalAnchor> {
        let fields = Fields::new(value)?;
        let range = |v: &Value| parse_int_range(v, DIMENSION_MIN_Y, DIMENSION_MAX_Y);
        let candidates = [
            fields.optional("absolute", range)?.map(VerticalAnchor::Absolute),
            fields.optional("above_bottom", range)?.map(VerticalAnchor::AboveBottom),
            fields.optional("below_top", range)?.map(VerticalAnchor::BelowTop),
            fields
                .optional("relative_to_sea_level", range)?
                .map(VerticalAnchor::RelativeToSeaLevel),
        ];
        let mut found = candidates.into_iter().flatten();
        match (found.next(), found.next()) {
            (Some(anchor), None) => Ok(anchor),
            (None, _) => Err(LoadError::new("Failed to parse vertical anchor: no matching alternative")),
            (Some(_), Some(_)) => Err(LoadError::new("Both alternatives read successfully")),
        }
    }
}
