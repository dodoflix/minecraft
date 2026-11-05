//! `net.minecraft.world.level.block.SnowLayerBlock`.
use super::prelude::*;

pub struct SnowLayerBlock;

impl BlockBehaviour for SnowLayerBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        if below_state.is_tag("cannot_support_snow_layer") {
            false
        } else if below_state.is_tag("support_override_snow_layer") {
            true
        } else {
            helpers::is_face_full(&below_state.get_collision_shape(level, pos.below()), Direction::Up)
                || (below_state.is_block(state.block()) && below_state.get(props::LAYERS) == 8)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("SnowLayerBlock", |_| Box::new(SnowLayerBlock));
