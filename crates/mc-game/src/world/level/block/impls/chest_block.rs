//! `net.minecraft.world.level.block.ChestBlock` (extends `AbstractChestBlock`; trapped chest).
use super::prelude::*;
use mc_data::enums::ChestType;

pub struct ChestBlock;

/// `ChestType.getOpposite`.
fn opposite(ty: ChestType) -> ChestType {
    match ty {
        ChestType::Single => ChestType::Single,
        ChestType::Left => ChestType::Right,
        ChestType::Right => ChestType::Left,
    }
}

/// `ChestBlock.getConnectedDirection`.
pub fn get_connected_direction(state: BlockState) -> Direction {
    let facing = state.get(props::HORIZONTAL_FACING);
    if state.get(props::CHEST_TYPE) == ChestType::Left { facing.get_clockwise() } else { facing.get_counter_clockwise() }
}

/// `ChestBlock.updateShape` (`can_connect` is the virtual `chestCanConnectTo`).
pub fn update_shape(
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
    can_connect: &dyn Fn(BlockState) -> bool,
) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    if can_connect(neighbour_state) && direction_to_neighbour.get_axis().is_horizontal() {
        let neighbour_type = neighbour_state.get(props::CHEST_TYPE);
        if state.get(props::CHEST_TYPE) == ChestType::Single
            && neighbour_type != ChestType::Single
            && state.get(props::HORIZONTAL_FACING) == neighbour_state.get(props::HORIZONTAL_FACING)
            && get_connected_direction(neighbour_state) == direction_to_neighbour.get_opposite()
        {
            return state.set(props::CHEST_TYPE, opposite(neighbour_type));
        }
    } else if get_connected_direction(state) == direction_to_neighbour {
        return state.set(props::CHEST_TYPE, ChestType::Single);
    }
    state
}

impl BlockBehaviour for ChestBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        update_shape(state, level, pos, direction, neighbour_state, &|n| n.is_block(state.block()))
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ChestBlock", |_| Box::new(ChestBlock));
