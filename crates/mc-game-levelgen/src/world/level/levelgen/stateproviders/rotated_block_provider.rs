//! `RotatedBlockProvider` (`rotated`).

use super::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::Holder;
use crate::world::level::levelgen::codec::{TypeEntry, parse_direction};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::props;
use std::sync::Arc;

pub struct RotatedBlockProvider {
    pub state: Holder<dyn BlockStateProvider>,
    pub direction: Option<Direction>,
}

impl BlockStateProvider for RotatedBlockProvider {
    fn get_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        // the random direction is drawn before the wrapped provider runs
        let direction = self.direction.unwrap_or_else(|| Direction::get_random(random));
        let mut new_state = self.state.value().get_state(level, random, pos);
        new_state = new_state.try_set(props::AXIS, direction.get_axis()).unwrap_or(new_state);
        new_state = new_state.try_set(props::FACING, direction).unwrap_or(new_state);
        if direction.get_axis().is_horizontal() {
            new_state = new_state.try_set(props::HORIZONTAL_FACING, direction).unwrap_or(new_state);
        }
        new_state
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static ROTATED: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "rotated",
    decode: |fields, ctx| {
        let state = fields.required("state", |v| ctx.block_state_provider(v))?;
        let direction = fields.optional("direction", parse_direction)?;
        Ok(Arc::new(RotatedBlockProvider { state, direction }))
    },
};
