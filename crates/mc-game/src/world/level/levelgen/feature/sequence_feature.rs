//! `SequenceFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::placement::{PlacedFeature, decode_placed_feature_set};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct SequenceFeature {
    pub features: Vec<Holder<PlacedFeature>>,
}

impl SequenceFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(SequenceFeature { features: fields.required("features", |v| decode_placed_feature_set(v, ctx, true))? }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static SEQUENCE: TypeEntry<dyn Feature> = TypeEntry { name: "sequence", decode: SequenceFeature::decode };

impl Feature for SequenceFeature {
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        self.features.iter().flat_map(|f| f.value().get_features()).collect()
    }

    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        for feature in &self.features {
            if !feature.value().place(level, generator, random, origin) {
                return false;
            }
        }
        true
    }
}
