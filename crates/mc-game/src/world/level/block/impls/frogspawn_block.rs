//! `net.minecraft.world.level.block.FrogspawnBlock`.
use super::prelude::*;

pub struct FrogspawnBlock;

fn may_place_on(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let fluid_state = level.get_fluid_state(pos);
    let fluid_above = level.get_fluid_state(pos.above());
    (fluid_state.is_tag("supports_frogspawn") || level.get_block_state(pos).is_tag("supports_frogspawn")) && fluid_above.is_empty()
}

impl BlockBehaviour for FrogspawnBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        may_place_on(level, pos.below())
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !self.can_survive(state, &*level, pos) { air() } else { state }
    });
}

register_block_class!("FrogspawnBlock", |_| Box::new(FrogspawnBlock));
