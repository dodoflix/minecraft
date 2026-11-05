//! `treedecorators.AlterGroundDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::tree_decorator::{Context, TreeDecorator};
use super::tree_feature::get_lowest_trunk_or_root_of_tree;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_core::pos::BlockPos;
use std::sync::Arc;

pub struct AlterGroundDecorator {
    provider: Holder<dyn BlockStateProvider>,
}

impl AlterGroundDecorator {
    fn place_circle(&self, context: &mut Context<'_>, pos: BlockPos) {
        for xx in -2..=2i32 {
            for zz in -2..=2i32 {
                if xx.abs() != 2 || zz.abs() != 2 {
                    self.place_block_at(context, pos.offset(xx, 0, zz));
                }
            }
        }
    }

    fn place_block_at(&self, context: &mut Context<'_>, pos: BlockPos) {
        for dy in (-3..=2).rev() {
            let cursor = pos.above_n(dy);
            if let Some(replace_with) = self.provider.value().get_optional_state(context.level, context.random, cursor) {
                context.set_block(cursor, replace_with);
                break;
            }
            if !context.is_air(cursor) && dy < 0 {
                break;
            }
        }
    }
}

impl TreeDecorator for AlterGroundDecorator {
    fn place(&self, context: &mut Context<'_>) {
        let block_positions = get_lowest_trunk_or_root_of_tree(context);
        let Some(first) = block_positions.first() else { return };
        let min_y = first.y();
        for pos in block_positions.into_iter().filter(|pos| pos.y() == min_y) {
            self.place_circle(context, pos.west().north());
            self.place_circle(context, pos.east_n(2).north());
            self.place_circle(context, pos.west().south_n(2));
            self.place_circle(context, pos.east_n(2).south_n(2));

            for _ in 0..5 {
                let placement = context.random.next_int_bounded(64);
                let xx = placement % 8;
                let zz = placement / 8;
                if xx == 0 || xx == 7 || zz == 0 || zz == 7 {
                    self.place_circle(context, pos.offset(-3 + xx, 0, -3 + zz));
                }
            }
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static ALTER_GROUND: TypeEntry<dyn TreeDecorator> =
    TypeEntry { name: "alter_ground", decode: |fields, ctx| Ok(Arc::new(AlterGroundDecorator { provider: fields.required("provider", |v| ctx.block_state_provider(v))? })) };
