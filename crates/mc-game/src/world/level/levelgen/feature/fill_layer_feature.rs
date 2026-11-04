//! `FillLayerFeature`: fills the air of the chunk-wide layer `minY + height` (flat world layers).

use super::Feature;
use crate::world::level::BlockGetter;
use crate::world::level::levelgen::codec::{TypeEntry, parse_block_state, parse_int_range};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_worldgen::noise_settings::DIMENSION_Y_SIZE;
use std::sync::Arc;

pub struct FillLayerFeature {
    pub height: i32,
    pub state: BlockState,
}

impl Feature for FillLayerFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        for dx in 0..16 {
            for dz in 0..16 {
                let pos = BlockPos::new(origin.x().wrapping_add(dx), level.get_min_y().wrapping_add(self.height), origin.z().wrapping_add(dz));
                if level.get_block_state(pos).is_air() {
                    level.set_block_flags(pos, self.state, 2);
                }
            }
        }
        true
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static FILL_LAYER: TypeEntry<dyn Feature> = TypeEntry {
    name: "fill_layer",
    decode: |fields, _| {
        Ok(Arc::new(FillLayerFeature {
            height: fields.required("height", |v| parse_int_range(v, 0, DIMENSION_Y_SIZE))?,
            state: fields.required("state", parse_block_state)?,
        }))
    },
};
