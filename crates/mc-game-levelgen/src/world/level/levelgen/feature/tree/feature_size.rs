//! `featuresize.FeatureSize`.

use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, parse_int_range};

pub trait FeatureSize: Send + Sync {
    /// `getSizeAtHeight(treeHeight, yo)`.
    fn get_size_at_height(&self, tree_height: i32, yo: i32) -> i32;
    /// `minClippedHeight()`.
    fn min_clipped_height(&self) -> Option<i32>;
}

/// `FeatureSize.minClippedHeightCodec()`: `intRange(0, 80).optionalFieldOf("min_clipped_height")`.
pub fn decode_min_clipped_height(fields: &Fields<'_>) -> LoadResult<Option<i32>> {
    fields.optional("min_clipped_height", |v| parse_int_range(v, 0, 80))
}
