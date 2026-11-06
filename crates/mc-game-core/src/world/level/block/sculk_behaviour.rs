//! `SculkBehaviour`: how a block takes part in a `SculkSpreader` (implemented by
//! `SculkBlock` and `SculkVeinBlock`; everything else uses [`DEFAULT`]).

use super::sculk_spreader::{ChargeCursor, SculkSpreader};
use crate::world::level::LevelAccessor;
use crate::world::level::block::impls::{sculk_block, sculk_vein_block};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;

pub trait SculkBehaviour: Send + Sync {
    /// `getSculkSpreadDelay()`.
    fn get_sculk_spread_delay(&self) -> u8 {
        1
    }

    /// `onDischarged(level, state, pos, random)`.
    fn on_discharged(
        &self,
        _level: &mut dyn LevelAccessor,
        _state: BlockState,
        _pos: BlockPos,
        _random: &mut dyn RandomSource,
    ) {
    }

    /// `depositCharge(level, pos, random)`.
    fn deposit_charge(
        &self,
        _level: &mut dyn LevelAccessor,
        _pos: BlockPos,
        _random: &mut dyn RandomSource,
    ) -> bool {
        false
    }

    /// `attemptSpreadVein(level, pos, state, facings, postProcess)`.
    fn attempt_spread_vein(
        &self,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        state: BlockState,
        _facings: Option<&[Direction]>,
        post_process: bool,
    ) -> bool {
        sculk_vein_block::vein_spreader().spread_all(state, level, pos, post_process) > 0
    }

    /// `canChangeBlockStateOnSpread()`.
    fn can_change_block_state_on_spread(&self) -> bool {
        true
    }

    /// `updateDecayDelay(age)`.
    fn update_decay_delay(&self, _age: i32) -> i32 {
        1
    }

    /// `attemptUseCharge(cursor, level, originPos, random, spreader, spreadVeins)`.
    fn attempt_use_charge(
        &self,
        cursor: &ChargeCursor,
        level: &mut dyn LevelAccessor,
        origin_pos: BlockPos,
        random: &mut dyn RandomSource,
        spreader: &SculkSpreader,
        spread_veins: bool,
    ) -> i32;
}

/// `SculkBehaviour.DEFAULT`.
pub struct DefaultSculkBehaviour;

pub static DEFAULT: DefaultSculkBehaviour = DefaultSculkBehaviour;

impl SculkBehaviour for DefaultSculkBehaviour {
    fn attempt_spread_vein(
        &self,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        state: BlockState,
        facings: Option<&[Direction]>,
        post_process: bool,
    ) -> bool {
        match facings {
            None => {
                let current = level.get_block_state(pos);
                sculk_vein_block::same_space_spreader().spread_all(
                    current,
                    level,
                    pos,
                    post_process,
                ) > 0
            }
            Some(faces) if !faces.is_empty() => {
                if !state.is_air() && state.get_fluid_state().fluid() != Fluid::WATER {
                    false
                } else {
                    sculk_vein_block::regrow(level, pos, state, faces)
                }
            }
            Some(_) => {
                sculk_vein_block::vein_spreader().spread_all(state, level, pos, post_process) > 0
            }
        }
    }

    fn attempt_use_charge(
        &self,
        cursor: &ChargeCursor,
        _level: &mut dyn LevelAccessor,
        _origin_pos: BlockPos,
        _random: &mut dyn RandomSource,
        _spreader: &SculkSpreader,
        _spread_veins: bool,
    ) -> i32 {
        if cursor.get_decay_delay() > 0 {
            cursor.get_charge()
        } else {
            0
        }
    }

    fn update_decay_delay(&self, age: i32) -> i32 {
        (age - 1).max(0)
    }
}

/// `block instanceof SculkBehaviour behaviour ? behaviour : null`.
pub fn sculk_behaviour_of(block: Block) -> Option<&'static dyn SculkBehaviour> {
    if block == Block::SCULK {
        Some(&sculk_block::BEHAVIOUR)
    } else if block == Block::SCULK_VEIN {
        Some(&sculk_vein_block::BEHAVIOUR)
    } else {
        None
    }
}
