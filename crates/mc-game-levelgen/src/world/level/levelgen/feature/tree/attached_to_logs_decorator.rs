//! `treedecorators.AttachedToLogsDecorator`.

use super::TREE_DECORATOR_TYPES;
use super::attached_to_leaves_decorator::parse_non_empty_directions;
use super::tree_decorator::{Context, TreeDecorator};
use mc_core::direction::Direction;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use std::sync::Arc;

pub struct AttachedToLogsDecorator {
    probability: f32,
    block_provider: Holder<dyn BlockStateProvider>,
    directions: Vec<Direction>,
}

impl TreeDecorator for AttachedToLogsDecorator {
    fn place(&self, context: &mut Context<'_>) {
        let mut logs = context.logs.clone();
        shuffle(&mut logs, context.random);
        for logs_pos in logs {
            let direction = self.directions[context
                .random
                .next_int_bounded(self.directions.len() as i32)
                as usize];
            let placement_pos = logs_pos.relative(direction);
            if context.random.next_float() <= self.probability && context.is_air(placement_pos) {
                let state = self.block_provider.value().get_state(
                    context.level,
                    context.random,
                    placement_pos,
                );
                context.set_block(placement_pos, state);
            }
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static ATTACHED_TO_LOGS: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "attached_to_logs",
    decode: |fields, ctx| {
        Ok(Arc::new(AttachedToLogsDecorator {
            probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))?,
            block_provider: fields.required("block_provider", |v| ctx.block_state_provider(v))?,
            directions: fields.required("directions", parse_non_empty_directions)?,
        }))
    },
};
