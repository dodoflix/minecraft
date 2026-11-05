//! `trunkplacers.TrunkPlacer`: the abstract class as a trait whose provided methods are vanilla's concrete ones.

use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{self, TreeFeature, TreeSetter, block_is_logs};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, parse_int_range};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;

/// The fields every trunk placer shares (`trunkPlacerParts`).
#[derive(Clone, Copy, Debug)]
pub struct TrunkBase {
    pub base_height: i32,
    pub height_rand_a: i32,
    pub height_rand_b: i32,
}

impl TrunkBase {
    /// `trunkPlacerParts(instance)`.
    pub fn decode(fields: &Fields<'_>) -> LoadResult<TrunkBase> {
        Ok(TrunkBase {
            base_height: fields.required("base_height", |v| parse_int_range(v, 0, 32))?,
            height_rand_a: fields.required("height_rand_a", |v| parse_int_range(v, 0, 24))?,
            height_rand_b: fields.required("height_rand_b", |v| parse_int_range(v, 0, 24))?,
        })
    }
}

pub trait TrunkPlacer: Send + Sync {
    fn base(&self) -> &TrunkBase;

    /// `placeTrunk(level, trunkSetter, random, treeHeight, origin, tree)`.
    fn place_trunk(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree_height: i32,
        origin: BlockPos,
        tree: &TreeFeature,
    ) -> Vec<FoliageAttachment>;

    /// `getBaseHeight()`.
    fn get_base_height(&self) -> i32 {
        self.base().base_height
    }

    /// `getTreeHeight(random)`.
    fn get_tree_height(&self, random: &mut dyn RandomSource) -> i32 {
        let base = self.base();
        let a = random.next_int_bounded(base.height_rand_a + 1);
        let b = random.next_int_bounded(base.height_rand_b + 1);
        base.base_height + a + b
    }

    /// `validTreePos(level, pos)`.
    fn valid_tree_pos(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        tree_feature::valid_tree_pos(level, pos)
    }

    /// `isFree(level, pos)`.
    fn is_free(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        self.valid_tree_pos(level, pos) || block_is_logs(level.get_block_state(pos))
    }

    /// `placeLog(level, trunkSetter, random, pos, tree)`.
    fn place_log(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        pos: BlockPos,
        tree: &TreeFeature,
    ) -> bool {
        self.place_log_with(level, trunk_setter, random, pos, tree, &|state| state)
    }

    /// `placeLog(.., stateModifier)`.
    fn place_log_with(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        pos: BlockPos,
        tree: &TreeFeature,
        state_modifier: &dyn Fn(BlockState) -> BlockState,
    ) -> bool {
        if self.valid_tree_pos(level, pos) {
            let state = tree.trunk_provider.value().get_state(level, random, pos);
            trunk_setter.set(level, pos, state_modifier(state));
            true
        } else {
            false
        }
    }

    /// `placeLogIfFree(level, trunkSetter, random, pos, tree)`.
    fn place_log_if_free(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        pos: BlockPos,
        tree: &TreeFeature,
    ) {
        if self.is_free(level, pos) {
            self.place_log(level, trunk_setter, random, pos, tree);
        }
    }
}

/// `TrunkPlacer.placeBelowTrunkBlock(level, trunkSetter, random, pos, tree)`.
pub fn place_below_trunk_block(
    level: &mut dyn WorldGenLevel,
    trunk_setter: &mut TreeSetter,
    random: &mut dyn RandomSource,
    pos: BlockPos,
    tree: &TreeFeature,
) {
    if let Some(state) = tree.below_trunk_provider.value().get_optional_state(level, random, pos) {
        trunk_setter.set(level, pos, state);
    }
}

/// `state.trySetValue(RotatedPillarBlock.AXIS, axis)`.
pub fn with_axis(state: BlockState, axis: mc_core::direction::Axis) -> BlockState {
    state.try_set(mc_data::props::AXIS, axis).unwrap_or(state)
}
