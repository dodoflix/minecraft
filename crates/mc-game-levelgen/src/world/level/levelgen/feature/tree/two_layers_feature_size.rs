//! `featuresize.TwoLayersFeatureSize`.

use super::FEATURE_SIZE_TYPES;
use super::feature_size::{FeatureSize, decode_min_clipped_height};
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_int_range};
use std::sync::Arc;

pub struct TwoLayersFeatureSize {
    limit: i32,
    lower_size: i32,
    upper_size: i32,
    min_clipped_height: Option<i32>,
}

impl FeatureSize for TwoLayersFeatureSize {
    fn get_size_at_height(&self, _tree_height: i32, yo: i32) -> i32 {
        if yo < self.limit {
            self.lower_size
        } else {
            self.upper_size
        }
    }

    fn min_clipped_height(&self) -> Option<i32> {
        self.min_clipped_height
    }
}

#[linkme::distributed_slice(FEATURE_SIZE_TYPES)]
static TWO_LAYERS_FEATURE_SIZE: TypeEntry<dyn FeatureSize> = TypeEntry {
    name: "two_layers_feature_size",
    decode: |fields, _| {
        Ok(Arc::new(TwoLayersFeatureSize {
            limit: fields
                .optional("limit", |v| parse_int_range(v, 0, 81))?
                .unwrap_or(1),
            lower_size: fields
                .optional("lower_size", |v| parse_int_range(v, 0, 16))?
                .unwrap_or(0),
            upper_size: fields
                .optional("upper_size", |v| parse_int_range(v, 0, 16))?
                .unwrap_or(1),
            min_clipped_height: decode_min_clipped_height(fields)?,
        }))
    },
};
