//! `WeightedRandomSelectorFeature`: one placed feature picked from a weighted list.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::Feature;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::PlacedFeature;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::WeightedList;
use std::sync::Arc;

pub struct WeightedRandomSelectorFeature {
    pub features: WeightedList<Holder<PlacedFeature>>,
}

impl Feature for WeightedRandomSelectorFeature {
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        self.features
            .unwrap()
            .iter()
            .flat_map(|weighted| weighted.value.value().get_features())
            .collect()
    }

    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        match self.features.get_random(random) {
            Some(feature) => feature.value().place(level, generator, random, origin),
            None => false,
        }
    }
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static WEIGHTED_RANDOM_SELECTOR: TypeEntry<dyn Feature> = TypeEntry {
    name: "weighted_random_selector",
    decode: |fields, ctx| {
        Ok(Arc::new(WeightedRandomSelectorFeature {
            features: fields.required("features", |v| {
                WeightedList::from_json(v, |e| ctx.placed_feature(e))
            })?,
        }))
    },
};
