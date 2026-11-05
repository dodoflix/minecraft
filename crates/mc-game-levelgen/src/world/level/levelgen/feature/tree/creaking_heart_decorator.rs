//! `treedecorators.CreakingHeartDecorator`.

use super::TREE_DECORATOR_TYPES;
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use super::tree_decorator::{Context, TreeDecorator};
use super::tree_feature::block_is_logs;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_core::direction::Direction;
use mc_data::block::Block;
use mc_data::enums::CreakingHeartState;
use mc_data::props;
use std::sync::Arc;

pub struct CreakingHeartDecorator {
    probability: f32,
}

impl TreeDecorator for CreakingHeartDecorator {
    fn place(&self, context: &mut Context<'_>) {
        if context.logs.is_empty() || context.random.next_float() >= self.probability {
            return;
        }
        let mut heart_placements = context.logs.clone();
        shuffle(&mut heart_placements, context.random);
        let target = heart_placements.into_iter().find(|pos| Direction::ALL.iter().all(|dir| context.check_block(pos.relative(*dir), block_is_logs)));
        if let Some(target) = target {
            context.set_block(
                target,
                Block::CREAKING_HEART.default_state().set(props::CREAKING_HEART_STATE, CreakingHeartState::Dormant).set(props::NATURAL, true),
            );
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static CREAKING_HEART: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "creaking_heart",
    decode: |fields, _| Ok(Arc::new(CreakingHeartDecorator { probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))? })),
};
