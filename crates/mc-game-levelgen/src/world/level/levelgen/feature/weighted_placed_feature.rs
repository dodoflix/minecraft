//! `WeightedPlacedFeature` (deprecated in vanilla): a placed feature with its selection chance, element of `random_selector`.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, parse_float_range};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::PlacedFeature;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use serde_json::Value;

pub struct WeightedPlacedFeature {
    pub feature: Holder<PlacedFeature>,
    pub chance: f32,
}

impl WeightedPlacedFeature {
    /// `WeightedPlacedFeature.CODEC`.
    pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<WeightedPlacedFeature> {
        let fields = Fields::new(value)?;
        Ok(WeightedPlacedFeature {
            feature: fields.required("feature", |v| ctx.placed_feature(v))?,
            chance: fields.required("chance", |v| parse_float_range(v, 0.0, 1.0))?,
        })
    }

    /// `place(level, chunkGenerator, random, origin)`.
    pub fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        self.feature.value().place(level, generator, random, origin)
    }
}
