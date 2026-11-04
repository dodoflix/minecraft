//! `net.minecraft.world.level.block.BaseCoralPlantTypeBlock` (also dead coral plants / fans: `BaseCoralPlantBlock`,
//! `BaseCoralFanBlock`).
use super::prelude::*;

pub struct BaseCoralPlantTypeBlock;

/// `scanForWater(state, level, pos)`.
pub fn scan_for_water(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
    if state.get(props::WATERLOGGED) {
        return true;
    }
    for direction in Direction::ALL {
        if level.get_fluid_state(pos.relative(direction)).is_tag("water") {
            return true;
        }
    }
    false
}

/// `tryScheduleDieTick(state, level, ticks, random, pos)` (`this` = the state's block, random = `level.random()`).
pub fn try_schedule_die_tick(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) {
    if !scan_for_water(state, &*level, pos) {
        let delay = 60 + level.random().next_int_bounded(40);
        helpers::schedule_block_tick(level, pos, state.block(), delay);
    }
}

/// `BaseCoralPlantTypeBlock.updateShape`.
pub fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, direction_to_neighbour: Direction) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    if direction_to_neighbour == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
}

/// `BaseCoralPlantTypeBlock.canSurvive`.
pub fn can_survive(level: &dyn LevelReader, pos: BlockPos) -> bool {
    let below = pos.below();
    level.get_block_state(below).is_face_sturdy_at(level, below, Direction::Up)
}

impl BlockBehaviour for BaseCoralPlantTypeBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        update_shape(state, level, pos, direction)
    });
}

register_block_class!("BaseCoralPlantTypeBlock", |_| Box::new(BaseCoralPlantTypeBlock));
