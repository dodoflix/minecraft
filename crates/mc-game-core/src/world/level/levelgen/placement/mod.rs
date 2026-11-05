//! `net.minecraft.world.level.levelgen.placement`: `PlacedFeature`, `PlacementModifier`s, `PlacementContext` and the
//! `FeaturePlacer` that drives them. Every modifier type lives in its own file and registers itself in
//! [`PLACEMENT_MODIFIER_TYPES`].

pub mod biome_filter;
pub mod block_predicate_filter;
pub mod cave_surface;
pub mod count_on_every_layer_placement;
pub mod count_placement;
pub mod environment_scan_placement;
pub mod fixed_placement;
pub mod height_range_placement;
pub mod heightmap_placement;
pub mod in_square_placement;
pub mod noise_based_count_placement;
pub mod noise_threshold_count_placement;
pub mod offset_placement;
pub mod random_chance_placement;
pub mod randomly_selected_placement;
pub mod rarity_filter;
pub mod repeating_placement;
pub mod surface_relative_threshold_filter;
pub mod surface_water_depth_filter;

use super::codec::{Fields, LoadError, LoadResult, TypeEntry, decode_dispatch, parse_list};
use super::env::GenEnv;
use super::feature::Feature;
use super::holder::Holder;
use super::registries::DecodeCtx;
use super::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::biome::PlacedFeatureRef;
use mc_worldgen::world_generation_context::WorldGenerationContext;
use serde_json::Value;
use std::sync::Arc;

/// `PlacementContext` (a `WorldGenerationContext` over the generator and the level).
pub struct PlacementContext<'a> {
    pub level: &'a dyn WorldGenLevel,
    pub generator: &'a GenEnv,
    /// `topFeature()`: the biome feature being placed (set for `placeWithBiomeCheck`).
    pub top_feature: Option<&'a PlacedFeatureRef>,
    pub world_generation_context: WorldGenerationContext,
}

impl<'a> PlacementContext<'a> {
    pub fn new(level: &'a dyn WorldGenLevel, generator: &'a GenEnv, top_feature: Option<&'a PlacedFeatureRef>) -> Self {
        let world_generation_context = generator.world_generation_context(level.level_height());
        Self { level, generator, top_feature, world_generation_context }
    }

    /// `getHeight(type, x, z)`.
    pub fn get_height(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        self.level.get_height_at(ty, x, z)
    }

    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        self.level.get_block_state(pos)
    }

    pub fn get_min_y(&self) -> i32 {
        self.level.get_min_y()
    }
}

/// `PlacementModifier`.
pub trait PlacementModifier: Send + Sync {
    /// `modify(context, random, origin, output)`: pushes the resulting positions to `output` in order.
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>);
}

/// `PlacementFilter`: keeps or drops the position. Implementors also implement [`PlacementModifier`] via
/// `fn modify(..) { filter_modify(self, ..) }`.
pub trait PlacementFilter {
    fn should_place(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, pos: BlockPos) -> bool;
}

/// `PlacementFilter.getPositions`.
pub fn filter_modify(filter: &impl PlacementFilter, context: &PlacementContext<'_>, random: &mut dyn RandomSource, pos: BlockPos, output: &mut Vec<BlockPos>) {
    if filter.should_place(context, random, pos) {
        output.push(pos);
    }
}

#[linkme::distributed_slice]
pub static PLACEMENT_MODIFIER_TYPES: [TypeEntry<dyn PlacementModifier>];

/// `PlacementModifier.CODEC`.
pub fn decode_modifier(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
    decode_dispatch(value, ctx, &PLACEMENT_MODIFIER_TYPES)
}

/// `PlacedFeature`.
pub struct PlacedFeature {
    pub feature: Holder<dyn Feature>,
    pub placement: Vec<Arc<dyn PlacementModifier>>,
}

impl PlacedFeature {
    /// `PlacedFeature.DIRECT_CODEC`.
    pub fn decode_direct(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<PlacedFeature> {
        let fields = Fields::new(value)?;
        Ok(PlacedFeature {
            feature: fields.required("feature", |v| ctx.feature(v))?,
            placement: fields.required("placement", |v| parse_list(v, |m| decode_modifier(m, ctx)))?,
        })
    }

    /// `place(level, generator, random, origin)`.
    pub fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        FeaturePlacer::new().place(self, level, generator, random, origin, None)
    }

    /// `getFeatures()`: this feature and its sub features.
    pub fn get_features(&self) -> Vec<Holder<dyn Feature>> {
        let mut out = vec![self.feature.clone()];
        out.extend(self.feature.value().get_sub_features());
        out
    }
}

/// `PlacedFeature.LIST_CODEC` (`HolderSet<PlacedFeature>`): a single element or a list of elements (`ExtraCodecs.compactListCodec`),
/// each a reference or an inline definition. `non_empty` is `ExtraCodecs.nonEmptyHolderSet`. A `#tag` set is an error: the
/// data pack has no `placed_feature` tags (the tag registry is not loaded).
pub fn decode_placed_feature_set(value: &Value, ctx: &DecodeCtx<'_>, non_empty: bool) -> LoadResult<Vec<Holder<PlacedFeature>>> {
    if let Value::String(s) = value
        && let Some(tag) = s.strip_prefix('#')
    {
        let id = mc_core::ident::Identifier::read(tag).map_err(LoadError::new)?;
        return Err(LoadError::new(format!("Missing tag: '{id}' in 'minecraft:root / minecraft:worldgen/placed_feature'")));
    }
    let holders = match value {
        Value::Array(_) => parse_list(value, |v| ctx.placed_feature(v))?,
        single => vec![ctx.placed_feature(single)?],
    };
    if non_empty && holders.is_empty() {
        return Err(LoadError::new("List must have contents"));
    }
    Ok(holders)
}

/// `FeaturePlacer`.
#[derive(Default)]
pub struct FeaturePlacer {
    positions: Vec<BlockPos>,
    modifier_indices: Vec<usize>,
    modified_positions: Vec<BlockPos>,
}

impl FeaturePlacer {
    pub fn new() -> FeaturePlacer {
        FeaturePlacer::default()
    }

    /// `placeWithBiomeCheck(placedFeature, random, origin)`: `key` identifies the placed feature in the biomes' feature lists.
    pub fn place_with_biome_check(
        &mut self,
        placed: &PlacedFeature,
        key: &PlacedFeatureRef,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        self.place(placed, level, generator, random, origin, Some(key))
    }

    fn place(
        &mut self,
        placed: &PlacedFeature,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        top_feature: Option<&PlacedFeatureRef>,
    ) -> bool {
        let feature = placed.feature.value().clone();
        let placement = &placed.placement;
        if placement.is_empty() {
            return feature.place(level, generator, random, origin);
        }
        let mut placed_any = false;
        self.positions.push(origin);
        self.modifier_indices.push(0);
        while !self.positions.is_empty() {
            let pos = self.positions.pop().expect("checked non-empty");
            let modifier_index = self.modifier_indices.pop().expect("parallel to positions");
            {
                let context = PlacementContext::new(&*level, generator, top_feature);
                placement[modifier_index].modify(&context, random, pos, &mut self.modified_positions);
            }
            let next = modifier_index + 1;
            if next < placement.len() {
                for i in (0..self.modified_positions.len()).rev() {
                    self.positions.push(self.modified_positions[i]);
                    self.modifier_indices.push(next);
                }
            } else {
                for i in 0..self.modified_positions.len() {
                    placed_any |= feature.place(level, generator, random, self.modified_positions[i]);
                }
            }
            self.modified_positions.clear();
        }
        placed_any
    }
}
