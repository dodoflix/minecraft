//! `net.minecraft.world.level.block.CarpetBlock` (also `WoolCarpetBlock`).
use super::prelude::*;

pub struct CarpetBlock;

impl BlockBehaviour for CarpetBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        !level.is_empty_block(pos.below())
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("CarpetBlock", |_| Box::new(CarpetBlock));
