//! `RandomBooleanSelectorFeature`: a coin flip between two placed features.

use mc_game_core::world::level::levelgen::feature::Feature;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::PlacedFeature;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct RandomBooleanSelectorFeature {
    pub feature_true: Holder<PlacedFeature>,
    pub feature_false: Holder<PlacedFeature>,
}

impl Feature for RandomBooleanSelectorFeature {
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        let mut out = self.feature_true.value().get_features();
        out.extend(self.feature_false.value().get_features());
        out
    }

    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let result = random.next_boolean();
        (if result { &self.feature_true } else { &self.feature_false }).value().place(level, generator, random, origin)
    }
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static RANDOM_BOOLEAN_SELECTOR: TypeEntry<dyn Feature> = TypeEntry {
    name: "random_boolean_selector",
    decode: |fields, ctx| {
        Ok(Arc::new(RandomBooleanSelectorFeature {
            feature_true: fields.required("feature_true", |v| ctx.placed_feature(v))?,
            feature_false: fields.required("feature_false", |v| ctx.placed_feature(v))?,
        }))
    },
};
