//! `net.minecraft.world.level.block.BambooSaplingBlock`.
use mc_data::enums::BambooLeaves;
use mc_data::item::Item;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct BambooSaplingBlock;

/// `growBamboo(level, pos)`.
fn grow_bamboo(level: &mut ServerLevel, pos: BlockPos) {
    level.set_block_and_update(
        pos.above(),
        Block::BAMBOO
            .default_state()
            .set(props::BAMBOO_LEAVES, BambooLeaves::Small),
    );
}

impl BlockBehaviour for BambooSaplingBlock {
    fn random_tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.random().next_int_bounded(3) == 0
            && level.is_empty_block(pos.above())
            && level.get_raw_brightness(pos.above(), 0) >= 9
        {
            grow_bamboo(level, pos);
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_tag("supports_bamboo")
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if !state.can_survive(&*level, pos) {
                air()
            } else if direction == Direction::Up && neighbour_state.is_block(Block::BAMBOO) {
                Block::BAMBOO.default_state()
            } else {
                state
            }
        }
    );
    fn get_clone_item_stack(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _include_data: bool,
    ) -> mc_game_core::world::item::ItemStack {
        mc_game_core::world::item::ItemStack::new(Item::BAMBOO)
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        level: &dyn LevelReader,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        level.get_block_state(pos.above()).is_air()
            && !level.is_outside_build_height(pos.above().y())
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) {
        grow_bamboo(level, pos);
    }
}

register_block_class!("BambooSaplingBlock", |_| Box::new(BambooSaplingBlock));
