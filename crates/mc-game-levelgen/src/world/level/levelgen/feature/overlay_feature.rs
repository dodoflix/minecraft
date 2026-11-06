//! `OverlayFeature`.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::{PlacedFeature, decode_placed_feature_set};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct OverlayFeature {
    pub features: Vec<Holder<PlacedFeature>>,
}

impl OverlayFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(OverlayFeature {
            features: fields.required("features", |v| decode_placed_feature_set(v, ctx, true))?,
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static OVERLAY: TypeEntry<dyn Feature> = TypeEntry {
    name: "overlay",
    decode: OverlayFeature::decode,
};

impl Feature for OverlayFeature {
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        self.features
            .iter()
            .flat_map(|f| f.value().get_features())
            .collect()
    }

    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let mut placed_any = false;
        for feature in &self.features {
            placed_any |= feature.value().place(level, generator, random, origin);
        }
        placed_any
    }
}
