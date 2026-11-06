//! `net.minecraft.world.level.block.SaplingBlock` (extends `VegetationBlock`): grows into the trees of its `TreeGrower`.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::grower::TreeGrower;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{
    MayPlaceOn, vegetation_behaviour_methods,
};

pub const BRIGHTNESS_FOR_SAPLING_GROWTH: i32 = 9;
pub const TICK_CHANCE_FOR_SAPLING_GROWTH: i32 = 7;

pub struct SaplingBlock {
    pub tree_grower: &'static TreeGrower,
}

impl MayPlaceOn for SaplingBlock {}

/// `SaplingBlock.advanceTree(level, pos, state, random)`.
pub fn advance_tree(
    tree_grower: &TreeGrower,
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
) {
    if state.get(props::STAGE) == 0 {
        level.set_block_flags(pos, state.cycle(props::STAGE), 260);
    } else {
        tree_grower.grow_tree(level, pos, state);
    }
}

impl BlockBehaviour for SaplingBlock {
    vegetation_behaviour_methods!();
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if helpers::get_max_local_raw_brightness(&*level, pos.above())
            >= BRIGHTNESS_FOR_SAPLING_GROWTH
            && level
                .random()
                .next_int_bounded(TICK_CHANCE_FOR_SAPLING_GROWTH)
                == 0
        {
            advance_tree(self.tree_grower, level, pos, state);
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        _level: &dyn LevelReader,
        _pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        false
    }
    fn is_valid_bonemeal_target_server(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        if !self.tree_grower.can_grow(level, pos, state) {
            false
        } else {
            let height_offset = self.tree_grower.get_minimum_height(level).unwrap_or(0);
            !level.is_outside_build_height(pos.above_n(height_offset).y())
        }
    }
    fn is_bonemeal_success(
        &self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        level.random().next_float() < 0.45
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) {
        advance_tree(self.tree_grower, level, pos, state);
    }
}

register_block_class!("SaplingBlock", |block| Box::new(SaplingBlock {
    tree_grower: match block.data().name {
        "minecraft:oak_sapling" => &TreeGrower::OAK,
        "minecraft:spruce_sapling" => &TreeGrower::SPRUCE,
        "minecraft:birch_sapling" => &TreeGrower::BIRCH,
        "minecraft:jungle_sapling" => &TreeGrower::JUNGLE,
        "minecraft:acacia_sapling" => &TreeGrower::ACACIA,
        "minecraft:cherry_sapling" => &TreeGrower::CHERRY,
        "minecraft:dark_oak_sapling" => &TreeGrower::DARK_OAK,
        "minecraft:pale_oak_sapling" => &TreeGrower::PALE_OAK,
        "minecraft:poplar_sapling" => &TreeGrower::POPLAR,
        name => panic!("unexpected SaplingBlock {name}"),
    },
}));
