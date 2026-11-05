//! `ChorusPlantFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::chorus_flower_block;
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct ChorusPlantFeature;

#[linkme::distributed_slice(FEATURE_TYPES)]
static CHORUS_PLANT: TypeEntry<dyn Feature> = TypeEntry { name: "chorus_plant", decode: |_, _| Ok(Arc::new(ChorusPlantFeature)) };

impl Feature for ChorusPlantFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        if level.is_empty_block(origin) && level.get_block_state(origin.below()).is_tag("supports_chorus_plant") {
            chorus_flower_block::generate_plant(level, origin, random, 8);
            true
        } else {
            false
        }
    }
}
