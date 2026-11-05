//! `treedecorators.TrunkVineDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::tree_decorator::{Context, TreeDecorator};
use crate::world::level::levelgen::codec::TypeEntry;
use mc_data::props;
use std::sync::Arc;

pub struct TrunkVineDecorator;

impl TreeDecorator for TrunkVineDecorator {
    fn place(&self, context: &mut Context<'_>) {
        for pos in context.logs.clone() {
            if context.random.next_int_bounded(3) > 0 {
                let west = pos.west();
                if context.is_air(west) {
                    context.place_vine(west, props::EAST);
                }
            }
            if context.random.next_int_bounded(3) > 0 {
                let east = pos.east();
                if context.is_air(east) {
                    context.place_vine(east, props::WEST);
                }
            }
            if context.random.next_int_bounded(3) > 0 {
                let north = pos.north();
                if context.is_air(north) {
                    context.place_vine(north, props::SOUTH);
                }
            }
            if context.random.next_int_bounded(3) > 0 {
                let south = pos.south();
                if context.is_air(south) {
                    context.place_vine(south, props::NORTH);
                }
            }
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static TRUNK_VINE: TypeEntry<dyn TreeDecorator> = TypeEntry { name: "trunk_vine", decode: |_, _| Ok(Arc::new(TrunkVineDecorator)) };
