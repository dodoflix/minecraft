//! `net.minecraft.world.level.block.SpeleothemBlock` (pointed dripstone, sulfur spike).
use super::prelude::*;
use mc_data::enums::SpeleothemThickness;

pub struct SpeleothemBlock;

/// `SpeleothemBlock.isSpeleothemWithDirection`.
pub fn is_speleothem_with_direction(block_state: BlockState, tip_direction: Direction) -> bool {
    block_state.is_tag("speleothems") && block_state.get(props::VERTICAL_DIRECTION) == tip_direction
}

/// `isValidSpeleothemPlacement` (`this` = the speleothem block).
pub fn is_valid_speleothem_placement(this: Block, level: &dyn LevelReader, pos: BlockPos, tip_direction: Direction) -> bool {
    let behind_pos = pos.relative(tip_direction.get_opposite());
    let behind_state = level.get_block_state(behind_pos);
    behind_state.is_face_sturdy_at(level, behind_pos, tip_direction) || (is_speleothem_with_direction(behind_state, tip_direction) && behind_state.is_block(this))
}

/// `calculateSpeleothemThickness`.
pub fn calculate_speleothem_thickness(this: Block, level: &dyn LevelReader, pos: BlockPos, tip_direction: Direction, merge_opposing_tips: bool) -> SpeleothemThickness {
    let base_direction = tip_direction.get_opposite();
    let in_front_state = level.get_block_state(pos.relative(tip_direction));
    if is_speleothem_with_direction(in_front_state, base_direction) && in_front_state.is_block(this) {
        if !merge_opposing_tips && in_front_state.get(props::SPELEOTHEM_THICKNESS) != SpeleothemThickness::TipMerge {
            SpeleothemThickness::Tip
        } else {
            SpeleothemThickness::TipMerge
        }
    } else if !is_speleothem_with_direction(in_front_state, tip_direction) {
        SpeleothemThickness::Tip
    } else {
        let in_front_thickness = in_front_state.get(props::SPELEOTHEM_THICKNESS);
        if in_front_thickness != SpeleothemThickness::Tip && in_front_thickness != SpeleothemThickness::TipMerge {
            let behind_state = level.get_block_state(pos.relative(base_direction));
            if !is_speleothem_with_direction(behind_state, tip_direction) { SpeleothemThickness::Base } else { SpeleothemThickness::Middle }
        } else {
            SpeleothemThickness::Frustum
        }
    }
}

impl BlockBehaviour for SpeleothemBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        is_valid_speleothem_placement(state.block(), level, pos, state.get(props::VERTICAL_DIRECTION))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction != Direction::Up && direction != Direction::Down {
            return state;
        }
        let tip_direction = state.get(props::VERTICAL_DIRECTION);
        if tip_direction == Direction::Down && level.has_scheduled_block_tick(pos, state.block()) {
            state
        } else if direction == tip_direction.get_opposite() && !self.can_survive(state, &*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), if tip_direction == Direction::Down { 2 } else { 1 });
            state
        } else {
            let merge_opposing_tips = state.get(props::SPELEOTHEM_THICKNESS) == SpeleothemThickness::TipMerge;
            state.set(props::SPELEOTHEM_THICKNESS, calculate_speleothem_thickness(state.block(), &*level, pos, tip_direction, merge_opposing_tips))
        }
    });
}

register_block_class!("SpeleothemBlock", |_| Box::new(SpeleothemBlock));
