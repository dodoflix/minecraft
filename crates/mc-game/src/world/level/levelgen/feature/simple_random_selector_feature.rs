//! `SimpleRandomSelectorFeature`: one uniformly chosen placed feature.

use super::Feature;
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::placement::{PlacedFeature, decode_placed_feature_set};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct SimpleRandomSelectorFeature {
    pub features: Vec<Holder<PlacedFeature>>,
}

impl Feature for SimpleRandomSelectorFeature {
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        self.features.iter().flat_map(|f| f.value().get_features()).collect()
    }

    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let index = random.next_int_bounded(self.features.len() as i32);
        self.features[index as usize].value().place(level, generator, random, origin)
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static SIMPLE_RANDOM_SELECTOR: TypeEntry<dyn Feature> = TypeEntry {
    name: "simple_random_selector",
    decode: |fields, ctx| Ok(Arc::new(SimpleRandomSelectorFeature { features: fields.required("features", |v| decode_placed_feature_set(v, ctx, true))? })),
};
