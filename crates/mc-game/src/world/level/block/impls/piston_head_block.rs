//! `net.minecraft.world.level.block.PistonHeadBlock` (`piston` package).
use super::prelude::*;
use mc_data::enums::PistonType;

pub struct PistonHeadBlock;

fn is_fitting_base(arm_state: BlockState, potential_base: BlockState) -> bool {
    let base_block = if arm_state.get(props::PISTON_TYPE) == PistonType::Default { Block::PISTON } else { Block::STICKY_PISTON };
    potential_base.is_block(base_block) && potential_base.get(props::EXTENDED) && potential_base.get(props::FACING) == arm_state.get(props::FACING)
}

impl BlockBehaviour for PistonHeadBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let base = level.get_block_state(pos.relative(state.get(props::FACING).get_opposite()));
        is_fitting_base(state, base) || (base.is_block(Block::MOVING_PISTON) && base.get(props::FACING) == state.get(props::FACING))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_opposite() == state.get(props::FACING) && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("PistonHeadBlock", |_| Box::new(PistonHeadBlock));
