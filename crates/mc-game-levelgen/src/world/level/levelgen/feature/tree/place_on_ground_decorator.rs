//! `treedecorators.PlaceOnGroundDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::tree_decorator::{Context, TreeDecorator};
use super::tree_feature::get_lowest_trunk_or_root_of_tree;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_int_range, parse_positive_int};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct PlaceOnGroundDecorator {
    tries: i32,
    radius: i32,
    height: i32,
    block_state_provider: Holder<dyn BlockStateProvider>,
}

impl PlaceOnGroundDecorator {
    fn attempt_to_place_block_above(&self, context: &mut Context<'_>, pos: BlockPos) {
        let above_pos = pos.above();
        let above = context.level.get_block_state(above_pos);
        if (above.is_air() || above.block() == Block::VINE)
            && context.check_block(pos, |state| state.is_solid_render())
            && context.level.get_height_at(HeightmapType::MotionBlockingNoLeaves, pos.x(), pos.z()) <= above_pos.y()
        {
            let state = self.block_state_provider.value().get_state(context.level, context.random, above_pos);
            context.set_block(above_pos, state);
        }
    }
}

impl TreeDecorator for PlaceOnGroundDecorator {
    fn place(&self, context: &mut Context<'_>) {
        let block_positions = get_lowest_trunk_or_root_of_tree(context);
        let Some(&origin) = block_positions.first() else { return };
        let min_y = origin.y();
        let (mut min_x, mut max_x, mut min_z, mut max_z) = (origin.x(), origin.x(), origin.z(), origin.z());
        for position in &block_positions {
            if position.y() == min_y {
                min_x = min_x.min(position.x());
                max_x = max_x.max(position.x());
                min_z = min_z.min(position.z());
                max_z = max_z.max(position.z());
            }
        }

        let bb = BoundingBox::new(min_x, min_y, min_z, max_x, min_y, max_z).inflated_by_xyz(self.radius, self.height, self.radius);
        for _ in 0..self.tries {
            let x = context.random.next_int_between_inclusive(bb.min_x(), bb.max_x());
            let y = context.random.next_int_between_inclusive(bb.min_y(), bb.max_y());
            let z = context.random.next_int_between_inclusive(bb.min_z(), bb.max_z());
            self.attempt_to_place_block_above(context, BlockPos::new(x, y, z));
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static PLACE_ON_GROUND: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "place_on_ground",
    decode: |fields, ctx| {
        Ok(Arc::new(PlaceOnGroundDecorator {
            tries: fields.optional("tries", parse_positive_int)?.unwrap_or(128),
            radius: fields.optional("radius", |v| parse_int_range(v, 0, i32::MAX))?.unwrap_or(2),
            height: fields.optional("height", |v| parse_int_range(v, 0, i32::MAX))?.unwrap_or(1),
            block_state_provider: fields.required("block_state_provider", |v| ctx.block_state_provider(v))?,
        }))
    },
};
