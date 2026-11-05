//! `rootplacers.RootPlacer`: the abstract class as a trait whose provided methods are vanilla's concrete ones.

use super::above_root_placement::AboveRootPlacement;
use super::tree_feature::{self, TreeFeature, TreeSetter, fluid_is_water};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::codec::{Fields, LoadResult};
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::props;
use mc_worldgen::provider::IntProvider;

/// The fields every root placer shares (`rootPlacerParts`).
pub struct RootBase {
    pub trunk_offset_y: IntProvider,
    pub root_provider: Holder<dyn BlockStateProvider>,
    pub above_root_placement: Option<AboveRootPlacement>,
}

impl RootBase {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<RootBase> {
        Ok(RootBase {
            trunk_offset_y: fields.required("trunk_offset_y", IntProvider::from_json)?,
            root_provider: fields.required("root_provider", |v| ctx.block_state_provider(v))?,
            above_root_placement: fields.optional("above_root_placement", |v| AboveRootPlacement::decode(v, ctx))?,
        })
    }
}

pub trait RootPlacer: Send + Sync {
    fn base(&self) -> &RootBase;

    /// `placeRoots(level, rootSetter, random, origin, trunkOrigin, tree)`.
    fn place_roots(
        &self,
        level: &mut dyn WorldGenLevel,
        root_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        trunk_origin: BlockPos,
        tree: &TreeFeature,
    ) -> bool;

    /// `canPlaceRoot(level, pos)`.
    fn can_place_root(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        tree_feature::valid_tree_pos(level, pos)
    }

    /// `placeRoot(level, rootSetter, random, pos, tree)`.
    fn place_root(&self, level: &mut dyn WorldGenLevel, root_setter: &mut TreeSetter, random: &mut dyn RandomSource, pos: BlockPos, tree: &TreeFeature) {
        place_root_default(self, level, root_setter, random, pos, tree);
    }

    /// `getTrunkOrigin(origin, random)`.
    fn get_trunk_origin(&self, origin: BlockPos, random: &mut dyn RandomSource) -> BlockPos {
        origin.above_n(self.base().trunk_offset_y.sample(random))
    }
}

/// `RootPlacer.placeRoot` (the base implementation, callable from overrides).
pub fn place_root_default<P: RootPlacer + ?Sized>(
    placer: &P,
    level: &mut dyn WorldGenLevel,
    root_setter: &mut TreeSetter,
    random: &mut dyn RandomSource,
    pos: BlockPos,
    _tree: &TreeFeature,
) {
    if placer.can_place_root(level, pos) {
        let state = placer.base().root_provider.value().get_state(level, random, pos);
        let state = get_potentially_waterlogged_state(level, pos, state);
        root_setter.set(level, pos, state);
        if let Some(above_placement) = &placer.base().above_root_placement {
            let above = pos.above();
            if random.next_float() < above_placement.above_root_placement_chance && level.get_block_state(above).is_air() {
                let state = above_placement.above_root_provider.value().get_state(level, random, above);
                let state = get_potentially_waterlogged_state(level, above, state);
                root_setter.set(level, above, state);
            }
        }
    }
}

/// `RootPlacer.getPotentiallyWaterloggedState(level, pos, state)`.
pub fn get_potentially_waterlogged_state(level: &dyn LevelAccessor, pos: BlockPos, state: BlockState) -> BlockState {
    if state.has_property(props::WATERLOGGED) {
        state.set(props::WATERLOGGED, fluid_is_water(level.get_fluid_state(pos)))
    } else {
        state
    }
}
