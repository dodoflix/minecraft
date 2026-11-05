//! `net.minecraft.world.level.block.ConcretePowderBlock` (extends `FallingBlock`).
use super::prelude::*;
use super::falling_block;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

pub struct ConcretePowderBlock;

/// `ConcretePowderBlock.canSolidify`.
fn can_solidify(state: BlockState) -> bool {
    state.get_fluid_state().is_tag("water")
}

/// `ConcretePowderBlock.shouldSolidify` (also used by `onLand` with the replaced block).
pub fn should_solidify(level: &dyn BlockGetter, pos: BlockPos, replaced_block: BlockState) -> bool {
    can_solidify(replaced_block) || touches_liquid(level, pos)
}

/// `ConcretePowderBlock.touchesLiquid`.
fn touches_liquid(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let mut touches = false;
    let mut test_pos = pos;
    for direction in Direction::ALL {
        let mut block_state = level.get_block_state(test_pos);
        if direction != Direction::Down || can_solidify(block_state) {
            test_pos = pos.relative(direction);
            block_state = level.get_block_state(test_pos);
            if can_solidify(block_state) && !block_state.is_face_sturdy_at(level, pos, direction.get_opposite()) {
                touches = true;
                break;
            }
        }
    }
    touches
}

impl BlockBehaviour for ConcretePowderBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_block = level.get_block_state(pos);
        if should_solidify(&*level, pos, replaced_block) { Some(concrete_of(block).default_state()) } else { Some(block.default_state()) }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if touches_liquid(&*level, pos) {
            concrete_of(state.block()).default_state()
        } else {
            falling_block::update_shape(state, level, pos, 2)
        }
    });
}

/// The `concrete` block of a concrete powder (`white_concrete_powder` -> `white_concrete`).
pub fn concrete_of(powder: Block) -> Block {
    let name = powder.data().name.trim_end_matches("_powder");
    Block::by_name(name).expect("concrete block")
}

register_block_class!("ConcretePowderBlock", |_| Box::new(ConcretePowderBlock));
