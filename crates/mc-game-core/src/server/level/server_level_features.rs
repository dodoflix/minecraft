//! Running configured / placed features on a live level (`ConfiguredFeature.place(level, generator, random, pos)` with the
//! level's own random, as bone meal, saplings, mushrooms, ... do) and the biome's bone meal feature list.

use super::ServerLevel;
use crate::world::level::levelgen::decoration::inline_placed_feature;
use crate::world::level::levelgen::feature::Feature;
use crate::world::level::levelgen::{Holder, WorldGenLevel};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_worldgen::biome::PlacedFeatureRef;
use std::sync::Arc;

/// `FeatureTags.CAN_SPAWN_FROM_BONE_MEAL` (vanilla tag of the configured feature registry).
const CAN_SPAWN_FROM_BONE_MEAL: &str = "minecraft:can_spawn_from_bone_meal";

impl ServerLevel {
    /// `ConfiguredFeature.place(level, generator, level.getRandom(), pos)` for `feature`.
    pub fn place_feature_value(&mut self, feature: &dyn Feature, pos: BlockPos) -> bool {
        let env = self.gen_env().clone();
        let mut random = self.tick_state.random.clone();
        if !WorldGenLevel::ensure_can_write(self, pos) {
            return false;
        }
        feature.place(self, &env, &mut random, pos)
    }

    /// `registryAccess().lookupOrThrow(FEATURE).get(key)`: a configured feature by id (`huge_brown_mushroom` or
    /// `minecraft:huge_brown_mushroom`).
    pub fn lookup_feature(&self, id: &str) -> Option<Arc<dyn Feature>> {
        let id = Identifier::read(id).ok()?;
        self.gen_env()
            .registries
            .feature(&id)
            .map(|h| h.value().clone())
    }

    /// `registry.get(id).value().place(level, generator, level.getRandom(), pos)` for a configured feature id
    /// (`minecraft:nylium_bonemeal` or `nylium_bonemeal`); `false` when the id is not registered.
    pub fn place_feature(&mut self, id: &str, pos: BlockPos) -> bool {
        let Some(feature) = self.lookup_feature(id) else {
            return false;
        };
        self.place_feature_value(feature.as_ref(), pos)
    }

    /// `registry.get(id).value().place(level, generator, RandomSource.create(), pos)`: the placement draws from a fresh
    /// unique-seed random (the End dragon fight's podium, gateways and pillars), not from the level's. `false` when the id is
    /// not registered or the feature does not place.
    pub fn place_feature_with_unique_seed(&mut self, id: &str, pos: BlockPos) -> bool {
        let Some(feature) = self.lookup_feature(id) else {
            return false;
        };
        self.place_feature_value_with_unique_seed(feature.as_ref(), pos)
    }

    /// `feature.place(level, generator, RandomSource.create(), pos)` for a feature value.
    pub fn place_feature_value_with_unique_seed(
        &mut self,
        feature: &dyn Feature,
        pos: BlockPos,
    ) -> bool {
        let env = self.gen_env().clone();
        let mut random = mc_core::random::create_new();
        if !WorldGenLevel::ensure_can_write(self, pos) {
            return false;
        }
        feature.place(self, &env, &mut random, pos)
    }

    /// The same for a placed feature id (`VegetationPlacements.GRASS_BONEMEAL`).
    pub fn place_placed_feature(&mut self, id: &str, pos: BlockPos) -> bool {
        let Ok(id) = Identifier::read(id) else {
            return false;
        };
        let env = self.gen_env().clone();
        let Some(placed) = env
            .registries
            .placed_feature(&id)
            .map(|h| h.value().clone())
        else {
            return false;
        };
        let mut random = self.tick_state.random.clone();
        if !WorldGenLevel::ensure_can_write(self, pos) {
            return false;
        }
        placed.place(self, &env, &mut random, pos)
    }

    /// `getBiome(pos).value().getGenerationSettings().getBoneMealFeatures()`: the biome's features (and their sub
    /// features) tagged `#minecraft:can_spawn_from_bone_meal`, in decoration order.
    pub fn get_bone_meal_features(&self, pos: BlockPos) -> Vec<Arc<dyn Feature>> {
        let env = self.gen_env();
        let tagged =
            mc_data::tags::names("worldgen/feature", CAN_SPAWN_FROM_BONE_MEAL).unwrap_or_default();
        let biome = WorldGenLevel::get_biome(self, pos);
        let mut out = Vec::new();
        for step in env.get_biome_generation_settings(biome).features() {
            for reference in step {
                let placed = match reference {
                    PlacedFeatureRef::Named(id) => env
                        .registries
                        .placed_feature(id)
                        .map(|holder| holder.value().clone()),
                    PlacedFeatureRef::Inline(inline) => inline_placed_feature(inline),
                    PlacedFeatureRef::InlineFillLayer { .. } => None,
                };
                let Some(placed) = placed else {
                    continue;
                };
                for holder in placed.get_features() {
                    if is_in_tag(&holder, &tagged) {
                        out.push(holder.value().clone());
                    }
                }
            }
        }
        out
    }
}

fn is_in_tag(holder: &Holder<dyn Feature>, tagged: &[String]) -> bool {
    holder.id().is_some_and(|id| {
        let id = id.to_string();
        tagged.contains(&id)
    })
}
