//! `treedecorators.LeaveVineDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::tree_decorator::{Context, TreeDecorator};
use crate::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_core::pos::BlockPos;
use mc_data::props::{self, BooleanProperty};
use std::sync::Arc;

pub struct LeaveVineDecorator {
    probability: f32,
}

fn add_hanging_vine(pos: BlockPos, direction: BooleanProperty, context: &mut Context<'_>) {
    context.place_vine(pos, direction);
    let mut max_dir = 4;
    let mut below = pos.below();
    while context.is_air(below) && max_dir > 0 {
        context.place_vine(below, direction);
        below = below.below();
        max_dir -= 1;
    }
}

impl TreeDecorator for LeaveVineDecorator {
    fn place(&self, context: &mut Context<'_>) {
        for pos in context.leaves.clone() {
            if context.random.next_float() < self.probability {
                let west = pos.west();
                if context.is_air(west) {
                    add_hanging_vine(west, props::EAST, context);
                }
            }
            if context.random.next_float() < self.probability {
                let east = pos.east();
                if context.is_air(east) {
                    add_hanging_vine(east, props::WEST, context);
                }
            }
            if context.random.next_float() < self.probability {
                let north = pos.north();
                if context.is_air(north) {
                    add_hanging_vine(north, props::SOUTH, context);
                }
            }
            if context.random.next_float() < self.probability {
                let south = pos.south();
                if context.is_air(south) {
                    add_hanging_vine(south, props::NORTH, context);
                }
            }
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static LEAVE_VINE: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "leave_vine",
    decode: |fields, _| Ok(Arc::new(LeaveVineDecorator { probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))? })),
};
