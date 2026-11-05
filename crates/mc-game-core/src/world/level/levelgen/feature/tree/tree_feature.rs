//! `levelgen.feature.TreeFeature` (type `tree`): the config record, the `place`/`doPlace` pipeline, `updateLeaves` and the
//! shared helpers (`validTreePos`, `isVine`, ...).

use super::java_hash_set::BlockPosHashSet;
use super::tree_decorator::{Context, TreeDecorator};
use super::{FEATURE_SIZE_TYPES, FOLIAGE_PLACER_TYPES, ROOT_PLACER_TYPES, TREE_DECORATOR_TYPES, TRUNK_PLACER_TYPES, block_tag_fn};
use super::feature_size::FeatureSize;
use super::foliage_placer::FoliagePlacer;
use super::root_placer::RootPlacer;
use super::trunk_placer::TrunkPlacer;
use crate::world::level::levelgen::codec::{LoadResult, TypeEntry, decode_dispatch, parse_bool, parse_list};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::structure::templatesystem::structure_template::update_shape_at_edge;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::LevelAccessor;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::phys::shapes::{BitSetDiscreteVoxelShape, DiscreteVoxelShape};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::{Fluid, FluidState};
use mc_data::props;
use std::collections::HashSet;
use std::sync::Arc;

block_tag_fn!(
    /// `BlockTags.LOGS`.
    block_is_logs, "minecraft:logs"
);
block_tag_fn!(
    /// `BlockTags.LEAVES`.
    block_is_leaves, "minecraft:leaves"
);
block_tag_fn!(
    /// `BlockTags.REPLACEABLE_BY_TREES`.
    block_is_replaceable_by_trees, "minecraft:replaceable_by_trees"
);
block_tag_fn!(
    /// `BlockTags.PREVENTS_NEARBY_LEAF_DECAY`.
    block_prevents_nearby_leaf_decay, "minecraft:prevents_nearby_leaf_decay"
);

/// `fluidState.is(FluidTags.WATER)`.
pub fn fluid_is_water(fluid: FluidState) -> bool {
    static TAG: std::sync::OnceLock<Option<&'static mc_data::tags::TagSet<Fluid>>> = std::sync::OnceLock::new();
    TAG.get_or_init(|| mc_data::tags::get::<Fluid>("minecraft:water")).is_some_and(|tag| tag.contains(fluid.fluid()))
}

/// `fluidState.isSourceOfType(Fluids.WATER)`.
pub fn fluid_is_water_source(fluid: FluidState) -> bool {
    fluid.is_source() && fluid.fluid() == Fluid::WATER
}

/// `TreeFeature.isVine(level, pos)`.
pub fn is_vine(level: &dyn LevelAccessor, pos: BlockPos) -> bool {
    level.get_block_state(pos).block() == Block::VINE
}

/// `TreeFeature.isAirOrLeaves(level, pos)`.
pub fn is_air_or_leaves(level: &dyn LevelAccessor, pos: BlockPos) -> bool {
    let state = level.get_block_state(pos);
    state.is_air() || block_is_leaves(state)
}

/// `TreeFeature.validTreePos(level, pos)`.
pub fn valid_tree_pos(level: &dyn LevelAccessor, pos: BlockPos) -> bool {
    let state = level.get_block_state(pos);
    state.is_air() || block_is_replaceable_by_trees(state)
}

/// The `BiConsumer<BlockPos, BlockState>` / `FoliageSetter` of the tree pipeline: writes the block (flags 19) and records
/// the position in a Java-ordered set. An untracked setter only writes (the fallen tree's decoration setter).
pub struct TreeSetter {
    positions: Option<BlockPosHashSet>,
}

impl TreeSetter {
    pub fn tracking() -> Self {
        TreeSetter { positions: Some(BlockPosHashSet::new()) }
    }

    pub fn untracked() -> Self {
        TreeSetter { positions: None }
    }

    /// `set(pos, state)`.
    pub fn set(&mut self, level: &mut dyn WorldGenLevel, pos: BlockPos, state: BlockState) {
        if let Some(positions) = &mut self.positions {
            positions.insert(pos);
        }
        level.set_block(pos, state, 19, 512);
    }

    /// `FoliageSetter.isSet(pos)`.
    pub fn is_set(&self, pos: &BlockPos) -> bool {
        self.positions.as_ref().is_some_and(|set| set.contains(pos))
    }

    pub fn positions(&self) -> BlockPosHashSet {
        self.positions.as_ref().map(|set| set.clone()).unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        self.positions.as_ref().is_none_or(|set| set.is_empty())
    }
}

pub struct TreeFeature {
    pub trunk_provider: Holder<dyn BlockStateProvider>,
    pub trunk_placer: Arc<dyn TrunkPlacer>,
    pub foliage_provider: Holder<dyn BlockStateProvider>,
    pub foliage_placer: Arc<dyn FoliagePlacer>,
    pub root_placer: Option<Arc<dyn RootPlacer>>,
    pub minimum_size: Arc<dyn FeatureSize>,
    pub decorators: Vec<Arc<dyn TreeDecorator>>,
    pub ignore_vines: bool,
    pub below_trunk_provider: Holder<dyn BlockStateProvider>,
}

impl TreeFeature {
    fn do_place(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        root_setter: &mut TreeSetter,
        trunk_setter: &mut TreeSetter,
        foliage_setter: &mut TreeSetter,
    ) -> bool {
        let tree_height = self.trunk_placer.get_tree_height(random);
        let foliage_height = self.foliage_placer.foliage_height(random, tree_height, self);
        let trunk_height = tree_height - foliage_height;
        let leaf_radius = self.foliage_placer.foliage_radius(random, trunk_height);
        let trunk_origin = match &self.root_placer {
            Some(root_placer) => root_placer.get_trunk_origin(origin, random),
            None => origin,
        };
        let min_y = origin.y().min(trunk_origin.y());
        let max_y = origin.y().max(trunk_origin.y()) + tree_height + 1;
        if min_y < level.get_min_y() + 1 || max_y > level.get_max_y() + 1 {
            return false;
        }
        let min_clipped_height = self.minimum_size.min_clipped_height();
        let clipped_tree_height = self.get_max_free_tree_height(level, tree_height, trunk_origin);
        if !(clipped_tree_height >= tree_height || min_clipped_height.is_some_and(|min| clipped_tree_height >= min)) {
            return false;
        }
        if let Some(root_placer) = &self.root_placer
            && !root_placer.place_roots(level, root_setter, random, origin, trunk_origin, self)
        {
            return false;
        }
        let foliage_attachments = self.trunk_placer.place_trunk(level, trunk_setter, random, clipped_tree_height, trunk_origin, self);
        for attachment in foliage_attachments {
            self.foliage_placer.create_foliage(level, foliage_setter, random, self, clipped_tree_height, attachment, foliage_height, leaf_radius);
        }
        true
    }

    fn get_max_free_tree_height(&self, level: &dyn LevelAccessor, max_tree_height: i32, tree_pos: BlockPos) -> i32 {
        for y in 0..=max_tree_height + 1 {
            let r = self.minimum_size.get_size_at_height(max_tree_height, y);
            for x in -r..=r {
                for z in -r..=r {
                    let block_pos = tree_pos.offset(x, y, z);
                    if !self.trunk_placer.is_free(level, block_pos) || !self.ignore_vines && is_vine(level, block_pos) {
                        return y - 2;
                    }
                }
            }
        }
        max_tree_height
    }

    pub fn decode(fields: &crate::world::level::levelgen::codec::Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<TreeFeature> {
        let trunk_provider = fields.required("trunk_provider", |v| ctx.block_state_provider(v))?;
        let trunk_placer = fields.required("trunk_placer", |v| decode_dispatch(v, ctx, &TRUNK_PLACER_TYPES))?;
        let foliage_provider = fields.required("foliage_provider", |v| ctx.block_state_provider(v))?;
        let foliage_placer = fields.required("foliage_placer", |v| decode_dispatch(v, ctx, &FOLIAGE_PLACER_TYPES))?;
        let root_placer = fields.optional("root_placer", |v| decode_dispatch(v, ctx, &ROOT_PLACER_TYPES))?;
        let minimum_size = fields.required("minimum_size", |v| decode_dispatch(v, ctx, &FEATURE_SIZE_TYPES))?;
        let decorators = fields.required("decorators", |v| parse_list(v, |d| decode_dispatch(d, ctx, &TREE_DECORATOR_TYPES)))?;
        // `Codec.BOOL.fieldOf("ignore_vines").orElse(false)`: a missing or malformed field is `false`
        let ignore_vines = fields.get("ignore_vines").and_then(|v| parse_bool(v).ok()).unwrap_or(false);
        let below_trunk_provider = fields.required("below_trunk_provider", |v| ctx.block_state_provider(v))?;
        Ok(TreeFeature { trunk_provider, trunk_placer, foliage_provider, foliage_placer, root_placer, minimum_size, decorators, ignore_vines, below_trunk_provider })
    }
}

/// `LeavesBlock.getOptionalDistanceAt(state)`.
fn get_optional_distance_at(state: BlockState) -> Option<i32> {
    if block_prevents_nearby_leaf_decay(state) { Some(0) } else { state.try_get(props::DISTANCE) }
}

/// `TreeFeature.updateLeaves(level, bounds, logs, decorationSet, rootPositions)`.
fn update_leaves(
    level: &mut dyn WorldGenLevel,
    bounds: &BoundingBox,
    logs: &BlockPosHashSet,
    decoration_set: &BlockPosHashSet,
    root_positions: &BlockPosHashSet,
) -> DiscreteVoxelShape {
    let mut shape = BitSetDiscreteVoxelShape::new(bounds.get_x_span(), bounds.get_y_span(), bounds.get_z_span());
    let fill = |shape: &mut BitSetDiscreteVoxelShape, pos: &BlockPos| shape.fill(pos.x() - bounds.min_x(), pos.y() - bounds.min_y(), pos.z() - bounds.min_z());
    let mut to_check: Vec<BlockPosHashSet> = (0..7).map(|_| BlockPosHashSet::new()).collect();

    for pos in decoration_set.iter().chain(root_positions.iter()) {
        if bounds.is_inside(&pos) {
            fill(&mut shape, &pos);
        }
    }

    let mut smallest_distance = 0usize;
    for log in logs.iter() {
        to_check[0].insert(log);
    }

    // The order inside one distance bucket matters: a position queued at a higher distance too (by a neighbour popped before
    // it) is set again with the larger distance when that bucket is reached, so the iteration order of Java's HashSet decides.
    loop {
        while smallest_distance >= 7 || !to_check[smallest_distance].is_empty() {
            if smallest_distance >= 7 {
                return shape.into();
            }
            let pos = to_check[smallest_distance].iter().next().expect("checked non-empty");
            to_check[smallest_distance].remove(&pos);
            if bounds.is_inside(&pos) {
                if smallest_distance != 0 {
                    let state = level.get_block_state(pos);
                    level.set_block(pos, state.set(props::DISTANCE, smallest_distance as i32), 19, 512);
                }
                fill(&mut shape, &pos);

                for direction in Direction::ALL {
                    let neighbor_pos = pos.relative(direction);
                    if bounds.is_inside(&neighbor_pos)
                        && !shape.is_full(neighbor_pos.x() - bounds.min_x(), neighbor_pos.y() - bounds.min_y(), neighbor_pos.z() - bounds.min_z())
                        && let Some(distance) = get_optional_distance_at(level.get_block_state(neighbor_pos))
                    {
                        let new_distance = distance.min(smallest_distance as i32 + 1);
                        if new_distance < 7 {
                            to_check[new_distance as usize].insert(neighbor_pos);
                            smallest_distance = smallest_distance.min(new_distance as usize);
                        }
                    }
                }
            }
        }
        smallest_distance += 1;
    }
}

/// `TreeFeature.getLowestTrunkOrRootOfTree(context)`.
pub fn get_lowest_trunk_or_root_of_tree(context: &Context<'_>) -> Vec<BlockPos> {
    let roots = &context.roots;
    let logs = &context.logs;
    let mut block_positions = Vec::new();
    if roots.is_empty() {
        block_positions.extend_from_slice(logs);
    } else if !logs.is_empty() && roots[0].y() == logs[0].y() {
        block_positions.extend_from_slice(logs);
        block_positions.extend_from_slice(roots);
    } else {
        block_positions.extend_from_slice(roots);
    }
    block_positions
}

impl Feature for TreeFeature {
    fn tree_trunk_base_height(&self) -> Option<i32> {
        Some(self.trunk_placer.get_base_height())
    }

    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let mut roots = TreeSetter::tracking();
        let mut trunks = TreeSetter::tracking();
        let mut foliage = TreeSetter::tracking();
        let mut decorations = TreeSetter::tracking();
        let result = self.do_place(level, random, origin, &mut roots, &mut trunks, &mut foliage);
        if !(result && (!trunks.is_empty() || !foliage.is_empty())) {
            return false;
        }
        if !self.decorators.is_empty() {
            let mut context = Context::new(level, &mut decorations, random, &trunks.positions(), &foliage.positions(), &roots.positions());
            for decorator in &self.decorators {
                decorator.place(&mut context);
            }
        }

        let (root_set, trunk_set, foliage_set, decoration_set) = (roots.positions(), trunks.positions(), foliage.positions(), decorations.positions());
        let all = root_set.iter().chain(trunk_set.iter()).chain(foliage_set.iter()).chain(decoration_set.iter());
        match BoundingBox::encapsulating_positions(all) {
            Some(bounds) => {
                let shape = update_leaves(level, &bounds, &trunk_set, &decoration_set, &root_set);
                update_shape_at_edge(level, 3, &shape, bounds.min_x(), bounds.min_y(), bounds.min_z());
                true
            }
            None => false,
        }
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static TREE: TypeEntry<dyn Feature> = TypeEntry { name: "tree", decode: |fields, ctx| Ok(Arc::new(TreeFeature::decode(fields, ctx)?)) };

