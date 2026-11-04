//! `RandomSelectorFeature` (deprecated in vanilla): the first feature whose chance roll succeeds, else the default.

use super::Feature;
use super::weighted_placed_feature::WeightedPlacedFeature;
use crate::world::level::levelgen::codec::{TypeEntry, parse_list};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::placement::PlacedFeature;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct RandomSelectorFeature {
    pub features: Vec<WeightedPlacedFeature>,
    pub default_feature: Holder<PlacedFeature>,
}

impl Feature for RandomSelectorFeature {
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        let mut out: Vec<_> = self.features.iter().flat_map(|weighted| weighted.feature.value().get_features()).collect();
        out.extend(self.default_feature.value().get_features());
        out
    }

    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        for feature in &self.features {
            if random.next_float() < feature.chance {
                return feature.place(level, generator, random, origin);
            }
        }
        self.default_feature.value().place(level, generator, random, origin)
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static RANDOM_SELECTOR: TypeEntry<dyn Feature> = TypeEntry {
    name: "random_selector",
    decode: |fields, ctx| {
        Ok(Arc::new(RandomSelectorFeature {
            features: fields.required("features", |v| parse_list(v, |e| WeightedPlacedFeature::decode(e, ctx)))?,
            default_feature: fields.required("default", |v| ctx.placed_feature(v))?,
        }))
    },
};
