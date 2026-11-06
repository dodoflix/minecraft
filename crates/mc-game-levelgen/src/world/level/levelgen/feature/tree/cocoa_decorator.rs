//! `treedecorators.CocoaDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::tree_decorator::{Context, TreeDecorator};
use mc_core::direction::Plane;
use mc_data::block::Block;
use mc_data::props;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use std::sync::Arc;

pub struct CocoaDecorator {
    probability: f32,
}

impl TreeDecorator for CocoaDecorator {
    fn place(&self, context: &mut Context<'_>) {
        if context.random.next_float() >= self.probability {
            return;
        }
        let logs = context.logs.clone();
        let Some(first) = logs.first() else { return };
        let tree_y = first.y();
        for pos in logs.into_iter().filter(|pos| pos.y() - tree_y <= 2) {
            for direction in Plane::Horizontal {
                if context.random.next_float() <= 0.25f32 {
                    let opposite = direction.get_opposite();
                    let cocoa_pos = pos.offset(opposite.get_step_x(), 0, opposite.get_step_z());
                    if context.is_air(cocoa_pos) {
                        let age = context.random.next_int_bounded(3);
                        context.set_block(
                            cocoa_pos,
                            Block::COCOA
                                .default_state()
                                .set(props::AGE_2, age)
                                .set(props::HORIZONTAL_FACING, direction),
                        );
                    }
                }
            }
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static COCOA: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "cocoa",
    decode: |fields, _| {
        Ok(Arc::new(CocoaDecorator {
            probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))?,
        }))
    },
};
