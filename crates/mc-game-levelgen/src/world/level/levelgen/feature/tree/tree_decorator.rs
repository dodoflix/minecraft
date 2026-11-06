//! `treedecorators.TreeDecorator` and its `Context`.

use super::tree_feature::TreeSetter;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::props::BooleanProperty;
use mc_game_core::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;

pub trait TreeDecorator: Send + Sync {
    /// `place(context)`.
    fn place(&self, context: &mut Context<'_>);
}

/// `TreeDecorator.Context`. The position lists are the (Java `HashSet` ordered) sets stably sorted by Y.
pub struct Context<'a> {
    pub level: &'a mut dyn WorldGenLevel,
    decoration_setter: &'a mut TreeSetter,
    pub random: &'a mut dyn RandomSource,
    pub logs: Vec<BlockPos>,
    pub leaves: Vec<BlockPos>,
    pub roots: Vec<BlockPos>,
}

impl<'a> Context<'a> {
    pub fn new(
        level: &'a mut dyn WorldGenLevel,
        decoration_setter: &'a mut TreeSetter,
        random: &'a mut dyn RandomSource,
        trunk_set: &BlockPosHashSet,
        foliage_set: &BlockPosHashSet,
        root_set: &BlockPosHashSet,
    ) -> Context<'a> {
        let mut roots: Vec<BlockPos> = root_set.iter().collect();
        let mut logs: Vec<BlockPos> = trunk_set.iter().collect();
        let mut leaves: Vec<BlockPos> = foliage_set.iter().collect();
        logs.sort_by_key(|pos| pos.y());
        leaves.sort_by_key(|pos| pos.y());
        roots.sort_by_key(|pos| pos.y());
        Context {
            level,
            decoration_setter,
            random,
            logs,
            leaves,
            roots,
        }
    }

    /// `placeVine(pos, direction)`.
    pub fn place_vine(&mut self, pos: BlockPos, direction: BooleanProperty) {
        self.set_block(pos, Block::VINE.default_state().set(direction, true));
    }

    /// `setBlock(pos, state)`.
    pub fn set_block(&mut self, pos: BlockPos, state: BlockState) {
        self.decoration_setter.set(self.level, pos, state);
    }

    /// `isAir(pos)`.
    pub fn is_air(&self, pos: BlockPos) -> bool {
        self.level.get_block_state(pos).is_air()
    }

    /// `checkBlock(pos, predicate)`.
    pub fn check_block(&self, pos: BlockPos, predicate: impl Fn(BlockState) -> bool) -> bool {
        predicate(self.level.get_block_state(pos))
    }

    /// `isReplaceable(pos)`.
    pub fn is_replaceable(&self, pos: BlockPos) -> bool {
        self.check_block(pos, |state| state.can_be_replaced())
    }

    /// `isWaterOrWaterNearby(pos)`.
    pub fn is_water_or_water_nearby(&self, pos: BlockPos) -> bool {
        let water = |p: BlockPos| self.check_block(p, |state| state.block() == Block::WATER);
        water(pos)
            || water(pos.east())
            || water(pos.west())
            || water(pos.north())
            || water(pos.south())
    }
}
