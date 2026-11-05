//! `treedecorators.ShelfMushroomDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::tree_decorator::{Context, TreeDecorator};
use crate::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::props;
use std::sync::Arc;

const PER_SIDE_PLACEMENT_CHANCE: f32 = 0.25;

pub struct ShelfMushroomDecorator {
    placement_probability: f32,
}

fn place_on_standing_tree(context: &mut Context<'_>, logs: &[BlockPos]) {
    let first = Plane::Horizontal.get_random_direction(context.random);
    let directions = [first, first.get_clockwise()];
    let tree_base_y = logs[0].y();

    for &log_pos in logs {
        if is_within_decoratable_height(log_pos, tree_base_y) {
            for facing in directions {
                if !(context.random.next_float() > PER_SIDE_PLACEMENT_CHANCE) && try_place_mushroom_on_standing_tree(context, log_pos, facing) {
                    break;
                }
            }
        }
    }
}

fn place_on_fallen_log(context: &mut Context<'_>, logs: &[BlockPos]) {
    let directions = perpendicular_to_fallen_log(logs);
    for &log_pos in logs {
        for facing in directions {
            if !(context.random.next_float() > PER_SIDE_PLACEMENT_CHANCE) {
                try_place_mushroom_on_fallen_tree(context, log_pos, facing);
            }
        }
    }
}

fn try_place_mushroom_on_standing_tree(context: &mut Context<'_>, log_pos: BlockPos, facing: Direction) -> bool {
    let mushroom_pos = mushroom_pos_for(log_pos, facing);
    if !is_block_replaceable_with_shelf_mushroom(context, mushroom_pos) {
        false
    } else if has_shelf_mushroom_at(context, mushroom_pos.below()) {
        false
    } else {
        place_mushroom(context, mushroom_pos, facing);
        true
    }
}

fn try_place_mushroom_on_fallen_tree(context: &mut Context<'_>, log_pos: BlockPos, facing: Direction) {
    let mushroom_pos = mushroom_pos_for(log_pos, facing);
    if is_block_replaceable_with_shelf_mushroom(context, mushroom_pos)
        && !has_horizontally_adjacent_shelf_mushroom(context, mushroom_pos)
        && !has_horizontally_adjacent_shelf_mushroom(context, log_pos)
    {
        place_mushroom(context, mushroom_pos, facing);
    }
}

fn perpendicular_to_fallen_log(logs: &[BlockPos]) -> [Direction; 2] {
    let first = logs[0];
    let last = logs[logs.len() - 1];
    if first.x() != last.x() { [Direction::North, Direction::South] } else { [Direction::East, Direction::West] }
}

fn is_within_decoratable_height(pos: BlockPos, tree_base_y: i32) -> bool {
    let dy = pos.y() - tree_base_y;
    (1..=4).contains(&dy)
}

fn mushroom_pos_for(log_pos: BlockPos, facing: Direction) -> BlockPos {
    log_pos.offset(facing.get_step_x(), 0, facing.get_step_z())
}

fn place_mushroom(context: &mut Context<'_>, pos: BlockPos, facing: Direction) {
    let age = context.random.next_int_bounded(2);
    context.set_block(pos, Block::SHELF_MUSHROOM.default_state().set(props::AGE_1, age).set(props::HORIZONTAL_FACING, facing));
}

fn has_shelf_mushroom_at(context: &Context<'_>, pos: BlockPos) -> bool {
    context.check_block(pos, |state| state.block() == Block::SHELF_MUSHROOM)
}

fn has_horizontally_adjacent_shelf_mushroom(context: &Context<'_>, pos: BlockPos) -> bool {
    Plane::Horizontal.iter().any(|dir| has_shelf_mushroom_at(context, pos.relative(dir)))
}

/// `ShelfMushroomDecorator.isBlockReplaceableWithShelfMushroom(context, pos)`.
pub fn is_block_replaceable_with_shelf_mushroom(context: &Context<'_>, pos: BlockPos) -> bool {
    context.is_replaceable(pos) && !context.is_water_or_water_nearby(pos)
}

impl TreeDecorator for ShelfMushroomDecorator {
    fn place(&self, context: &mut Context<'_>) {
        if context.random.next_float() >= self.placement_probability {
            return;
        }
        let logs = context.logs.clone();
        if logs.is_empty() {
            return;
        }
        // a fallen log is flat: first and last (Y-sorted) log share their Y
        if logs[0].y() == logs[logs.len() - 1].y() {
            place_on_fallen_log(context, &logs);
        } else {
            place_on_standing_tree(context, &logs);
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static SHELF_MUSHROOM: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "shelf_mushroom",
    decode: |fields, _| Ok(Arc::new(ShelfMushroomDecorator { placement_probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))? })),
};

