//! `net.minecraft.world.level.block.PoweredRailBlock` (extends `BaseRailBlock`); powered and activator rails.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::base_rail_block::{self, is_slope, rail_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::signal_getter;
use mc_data::enums::RailShape;

pub struct PoweredRailBlock;

/// `PoweredRailBlock.findPoweredRailSignal`.
fn find_powered_rail_signal(level: &ServerLevel, pos: BlockPos, state: BlockState, forward: bool, search_depth: i32) -> bool {
    if search_depth >= 8 {
        return false;
    }
    let (mut x, mut y, mut z) = (pos.x(), pos.y(), pos.z());
    let mut check_below = true;
    let mut shape = state.get(props::RAIL_SHAPE_STRAIGHT);
    match shape {
        RailShape::NorthSouth => {
            if forward { z += 1 } else { z -= 1 }
        }
        RailShape::EastWest => {
            if forward { x -= 1 } else { x += 1 }
        }
        RailShape::AscendingEast => {
            if forward {
                x -= 1;
            } else {
                x += 1;
                y += 1;
                check_below = false;
            }
            shape = RailShape::EastWest;
        }
        RailShape::AscendingWest => {
            if forward {
                x -= 1;
                y += 1;
                check_below = false;
            } else {
                x += 1;
            }
            shape = RailShape::EastWest;
        }
        RailShape::AscendingNorth => {
            if forward {
                z += 1;
            } else {
                z -= 1;
                y += 1;
                check_below = false;
            }
            shape = RailShape::NorthSouth;
        }
        RailShape::AscendingSouth => {
            if forward {
                z += 1;
                y += 1;
                check_below = false;
            } else {
                z -= 1;
            }
            shape = RailShape::NorthSouth;
        }
        _ => {}
    }
    is_same_rail_with_power(level, BlockPos::new(x, y, z), state.block(), forward, search_depth, shape)
        || (check_below && is_same_rail_with_power(level, BlockPos::new(x, y - 1, z), state.block(), forward, search_depth, shape))
}

/// `PoweredRailBlock.isSameRailWithPower`.
fn is_same_rail_with_power(level: &ServerLevel, pos: BlockPos, this: Block, forward: bool, search_depth: i32, dir: RailShape) -> bool {
    let state = level.get_block_state(pos);
    if state.block() != this {
        return false;
    }
    let my_shape = state.get(props::RAIL_SHAPE_STRAIGHT);
    if dir == RailShape::EastWest && matches!(my_shape, RailShape::NorthSouth | RailShape::AscendingNorth | RailShape::AscendingSouth) {
        return false;
    }
    if dir == RailShape::NorthSouth && matches!(my_shape, RailShape::EastWest | RailShape::AscendingEast | RailShape::AscendingWest) {
        return false;
    }
    if !state.get(props::POWERED) {
        return false;
    }
    signal_getter::has_neighbor_signal(level, pos) || find_powered_rail_signal(level, pos, state, forward, search_depth + 1)
}

/// `PoweredRailBlock.updateState(state, level, pos, block)`.
fn update_state(state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block) {
    let is_powered = state.get(props::POWERED);
    let should_power = signal_getter::has_neighbor_signal(&*level, pos)
        || find_powered_rail_signal(level, pos, state, true, 0)
        || find_powered_rail_signal(level, pos, state, false, 0);
    if should_power != is_powered {
        level.set_block_and_update(pos, state.set(props::POWERED, should_power));
        level.update_neighbors_at(pos.below(), state.block());
        if is_slope(state.get(props::RAIL_SHAPE_STRAIGHT)) {
            level.update_neighbors_at(pos.above(), state.block());
        }
    }
}



rail_behaviour!(PoweredRailBlock, update_state, base_rail_block::on_place);

register_block_class!("PoweredRailBlock", |_| Box::new(PoweredRailBlock));
