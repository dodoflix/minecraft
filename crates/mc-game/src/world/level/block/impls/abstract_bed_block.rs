//! `net.minecraft.world.level.block.AbstractBedBlock` (`BedBlock`, `StrawBedBlock`).
use super::prelude::*;
use mc_data::enums::BedPart;

pub struct AbstractBedBlock;

/// `AbstractBedBlock.getNeighbourDirection`.
fn get_neighbour_direction(part: BedPart, facing: Direction) -> Direction {
    if part == BedPart::Foot { facing } else { facing.get_opposite() }
}

impl BlockBehaviour for AbstractBedBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == get_neighbour_direction(state.get(props::BED_PART), state.get(props::HORIZONTAL_FACING)) {
            if neighbour_state.is_block(state.block()) && neighbour_state.get(props::BED_PART) != state.get(props::BED_PART) {
                state.set(props::OCCUPIED, neighbour_state.get(props::OCCUPIED))
            } else {
                air()
            }
        } else {
            state
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("AbstractBedBlock", |_| Box::new(AbstractBedBlock));
