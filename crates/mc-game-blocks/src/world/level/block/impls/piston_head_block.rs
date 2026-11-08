//! `net.minecraft.world.level.block.PistonHeadBlock` (`piston` package).
use mc_data::Item;
use mc_data::enums::PistonType;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::{register_block_class, update_shape};

pub struct PistonHeadBlock;

fn is_fitting_base(arm_state: BlockState, potential_base: BlockState) -> bool {
    let base_block = if arm_state.get(props::PISTON_TYPE) == PistonType::Default {
        Block::PISTON
    } else {
        Block::STICKY_PISTON
    };
    potential_base.is_block(base_block)
        && potential_base.get(props::EXTENDED)
        && potential_base.get(props::FACING) == arm_state.get(props::FACING)
}

impl BlockBehaviour for PistonHeadBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let base = level.get_block_state(pos.relative(state.get(props::FACING).get_opposite()));
        is_fitting_base(state, base)
            || (base.is_block(Block::MOVING_PISTON)
                && base.get(props::FACING) == state.get(props::FACING))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction.get_opposite() == state.get(props::FACING)
                && !state.can_survive(&*level, pos)
            {
                air()
            } else {
                state
            }
        }
    );
    fn player_will_destroy(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
    ) -> BlockState {
        if player.prevents_block_drops() {
            let base_pos = pos.relative(state.get(props::FACING).get_opposite());
            if is_fitting_base(state, level.get_block_state(base_pos)) {
                level.destroy_block(base_pos, false, None, 512);
            }
        }
        mc_game_core::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        let base_pos = pos.relative(state.get(props::FACING).get_opposite());
        if is_fitting_base(state, level.get_block_state(base_pos)) {
            level.destroy_block(base_pos, true, None, 512);
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        if state.can_survive(&*level, pos) {
            let behind = state.get(props::FACING).get_opposite();
            level.neighbor_changed(
                pos.relative(behind),
                block,
                orientation.map(|o| o.with_front(behind)),
            );
        }
    }
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        ItemStack::new(if state.get(props::PISTON_TYPE) == PistonType::Sticky {
            Item::STICKY_PISTON
        } else {
            Item::PISTON
        })
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("PistonHeadBlock", |_| Box::new(PistonHeadBlock));
