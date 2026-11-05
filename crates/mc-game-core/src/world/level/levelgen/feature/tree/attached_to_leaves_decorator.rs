//! `treedecorators.AttachedToLeavesDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::shuffle;
use super::tree_decorator::{Context, TreeDecorator};
use crate::world::level::levelgen::codec::{LoadError, TypeEntry, parse_direction, parse_float_range, parse_int_range, parse_list};
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use std::collections::HashSet;
use std::sync::Arc;

pub struct AttachedToLeavesDecorator {
    probability: f32,
    exclusion_radius_xz: i32,
    exclusion_radius_y: i32,
    block_provider: Holder<dyn BlockStateProvider>,
    required_empty_blocks: i32,
    directions: Vec<Direction>,
}

impl AttachedToLeavesDecorator {
    fn has_required_empty_blocks(&self, context: &Context<'_>, leaf_pos: BlockPos, direction: Direction) -> bool {
        (1..=self.required_empty_blocks).all(|i| context.is_air(leaf_pos.relative_n(direction, i)))
    }
}

impl TreeDecorator for AttachedToLeavesDecorator {
    fn place(&self, context: &mut Context<'_>) {
        let mut propagule_blacklist: HashSet<BlockPos> = HashSet::new();
        let mut leaves = context.leaves.clone();
        shuffle(&mut leaves, context.random);

        for leaf_pos in leaves {
            let direction = self.directions[context.random.next_int_bounded(self.directions.len() as i32) as usize];
            let placement_pos = leaf_pos.relative(direction);
            if !propagule_blacklist.contains(&placement_pos)
                && context.random.next_float() < self.probability
                && self.has_required_empty_blocks(context, leaf_pos, direction)
            {
                let (xz, y) = (self.exclusion_radius_xz, self.exclusion_radius_y);
                for pos in BlockPos::between_closed(&placement_pos.offset(-xz, -y, -xz), &placement_pos.offset(xz, y, xz)) {
                    propagule_blacklist.insert(pos);
                }
                let state = self.block_provider.value().get_state(context.level, context.random, placement_pos);
                context.set_block(placement_pos, state);
            }
        }
    }
}

/// `ExtraCodecs.nonEmptyList(Direction.CODEC.listOf())`.
pub fn parse_non_empty_directions(value: &serde_json::Value) -> Result<Vec<Direction>, LoadError> {
    let list = parse_list(value, parse_direction)?;
    if list.is_empty() { Err(LoadError::new("List must have contents")) } else { Ok(list) }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static ATTACHED_TO_LEAVES: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "attached_to_leaves",
    decode: |fields, ctx| {
        Ok(Arc::new(AttachedToLeavesDecorator {
            probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))?,
            exclusion_radius_xz: fields.required("exclusion_radius_xz", |v| parse_int_range(v, 0, 16))?,
            exclusion_radius_y: fields.required("exclusion_radius_y", |v| parse_int_range(v, 0, 16))?,
            block_provider: fields.required("block_provider", |v| ctx.block_state_provider(v))?,
            required_empty_blocks: fields.required("required_empty_blocks", |v| parse_int_range(v, 1, 16))?,
            directions: fields.required("directions", parse_non_empty_directions)?,
        }))
    },
};
