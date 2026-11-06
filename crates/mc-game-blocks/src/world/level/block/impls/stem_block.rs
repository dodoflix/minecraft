//! `net.minecraft.world.level.block.StemBlock` (extends `VegetationBlock`): pumpkin and melon stems.
use mc_core::direction::Plane;
use mc_core::math::mth;
use mc_data::Item;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::crop_block::get_growth_speed;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{
    MayPlaceOn, vegetation_behaviour_methods,
};

pub struct StemBlock {
    /// `fruit`.
    fruit: Block,
    /// `attachedStem`.
    attached_stem: Block,
    /// `seed`.
    seed: Item,
    /// `stemSupportBlocks` (block tag).
    stem_support_blocks: &'static str,
    /// `fruitSupportBlocks` (block tag).
    fruit_support_blocks: &'static str,
}

impl MayPlaceOn for StemBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag(self.stem_support_blocks)
    }
}

impl BlockBehaviour for StemBlock {
    vegetation_behaviour_methods!();
    fn random_tick(&self, mut state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.get_raw_brightness(pos, 0) >= 9 {
            let growth_speed = get_growth_speed(state.block(), &*level, pos);
            if level
                .random()
                .next_int_bounded((25.0f32 / growth_speed) as i32 + 1)
                == 0
            {
                let age = state.get(props::AGE_7);
                if age < 7 {
                    state = state.set(props::AGE_7, age + 1);
                    level.set_block_flags(pos, state, 2);
                } else {
                    let direction = Plane::Horizontal.get_random_direction(level.random());
                    let relative = pos.relative(direction);
                    let state_below = level.get_block_state(relative.below());
                    if level.get_block_state(relative).is_air()
                        && state_below.is_tag(self.fruit_support_blocks)
                    {
                        level.set_block_and_update(relative, self.fruit.default_state());
                        level.set_block_and_update(
                            pos,
                            self.attached_stem
                                .default_state()
                                .set(props::HORIZONTAL_FACING, direction),
                        );
                    }
                }
            }
        }
    }
    fn get_clone_item_stack(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        ItemStack::new(self.seed)
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        _level: &dyn LevelReader,
        _pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        state.get(props::AGE_7) != 7
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) {
        let age = 7.min(state.get(props::AGE_7) + mth::next_int(level.random(), 2, 5));
        let new_state = state.set(props::AGE_7, age);
        level.set_block_flags(pos, new_state, 2);
        if age == 7 {
            new_state.random_tick(level, pos);
        }
    }
}

register_block_class!("StemBlock", |block| Box::new(match block.data().name {
    "minecraft:pumpkin_stem" => StemBlock {
        fruit: Block::PUMPKIN,
        attached_stem: Block::ATTACHED_PUMPKIN_STEM,
        seed: Item::PUMPKIN_SEEDS,
        stem_support_blocks: "supports_pumpkin_stem",
        fruit_support_blocks: "supports_pumpkin_stem_fruit",
    },
    "minecraft:melon_stem" => StemBlock {
        fruit: Block::MELON,
        attached_stem: Block::ATTACHED_MELON_STEM,
        seed: Item::MELON_SEEDS,
        stem_support_blocks: "supports_melon_stem",
        fruit_support_blocks: "supports_melon_stem_fruit",
    },
    name => panic!("unexpected StemBlock {name}"),
}));
