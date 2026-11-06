//! `SculkBlock`'s `SculkBehaviour` (the block behaviour itself, a `DropExperienceBlock`, is the plain class chain).

use crate::world::level::LevelAccessor;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::sculk_behaviour::SculkBehaviour;
use crate::world::level::block::sculk_spreader::{ChargeCursor, SculkSpreader};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_data::props;
use mc_protocol::packets::play::level::SoundSource;

pub const GROWTH_INHIBITOR_RANGE: i32 = 4;

pub struct SculkBlockBehaviour;

pub static BEHAVIOUR: SculkBlockBehaviour = SculkBlockBehaviour;

fn get_decay_penalty(
    spreader: &SculkSpreader,
    pos: BlockPos,
    origin_pos: BlockPos,
    charge: i32,
) -> i32 {
    let no_growth_radius = spreader.no_growth_radius();
    let outer_distance_squared =
        mth::square_f32(pos.dist_sqr(&origin_pos).sqrt() as f32 - no_growth_radius as f32);
    let max_reach_squared = mth::square_i32(24 - no_growth_radius);
    let distance_factor = 1.0f32.min(outer_distance_squared / max_reach_squared as f32);
    1.max((charge as f32 * distance_factor * 0.5) as i32)
}

fn get_random_growth_state(
    level: &dyn LevelAccessor,
    pos: BlockPos,
    random: &mut dyn RandomSource,
    is_world_gen: bool,
) -> BlockState {
    let state = if random.next_int_bounded(11) == 0 {
        Block::SCULK_SHRIEKER
            .default_state()
            .set(props::CAN_SUMMON, is_world_gen)
    } else {
        Block::SCULK_SENSOR.default_state()
    };
    if state.has_property(props::WATERLOGGED) && !level.get_fluid_state(pos).is_empty() {
        state.set(props::WATERLOGGED, true)
    } else {
        state
    }
}

fn can_place_growth(level: &dyn LevelAccessor, pos: BlockPos) -> bool {
    let state_above = level.get_block_state(pos.above());
    if state_above.is_air()
        || state_above.block() == Block::WATER
            && state_above.get_fluid_state().fluid() == Fluid::WATER
    {
        // `findBlocksIn(..).filterState(is SCULK_GROWTH_INHIBITORS).atMostMatched(2)`
        let mut matched = 0;
        for p in BlockPos::between_closed(
            &pos.offset(-GROWTH_INHIBITOR_RANGE, 0, -GROWTH_INHIBITOR_RANGE),
            &pos.offset(GROWTH_INHIBITOR_RANGE, 2, GROWTH_INHIBITOR_RANGE),
        ) {
            if level.get_block_state(p).is_tag("sculk_growth_inhibitors") {
                matched += 1;
                if matched > 2 {
                    return false;
                }
            }
        }
        true
    } else {
        false
    }
}

impl SculkBehaviour for SculkBlockBehaviour {
    fn attempt_use_charge(
        &self,
        cursor: &ChargeCursor,
        level: &mut dyn LevelAccessor,
        origin_pos: BlockPos,
        random: &mut dyn RandomSource,
        spreader: &SculkSpreader,
        _spread_vein: bool,
    ) -> i32 {
        let charge = cursor.get_charge();
        if charge != 0 && random.next_int_bounded(spreader.charge_decay_rate()) == 0 {
            let charge_pos = cursor.get_pos();
            let is_close_to_catalyst =
                charge_pos.closer_than(&origin_pos, f64::from(spreader.no_growth_radius()));
            if !is_close_to_catalyst && can_place_growth(&*level, charge_pos) {
                let xp_per_growth_spawn = spreader.growth_spawn_cost();
                if random.next_int_bounded(xp_per_growth_spawn) < charge {
                    let growth_placement = charge_pos.above();
                    let growth_state = get_random_growth_state(
                        &*level,
                        growth_placement,
                        random,
                        spreader.is_world_generation(),
                    );
                    level.set_block_and_update(growth_placement, growth_state);
                    level.play_sound(
                        None,
                        charge_pos,
                        growth_state.get_sound_type().place_sound,
                        SoundSource::Blocks,
                        1.0,
                        1.0,
                    );
                }
                0.max(charge - xp_per_growth_spawn)
            } else if random.next_int_bounded(spreader.additional_decay_rate()) != 0 {
                charge
            } else {
                charge
                    - if is_close_to_catalyst {
                        1
                    } else {
                        get_decay_penalty(spreader, charge_pos, origin_pos, charge)
                    }
            }
        } else {
            charge
        }
    }

    fn can_change_block_state_on_spread(&self) -> bool {
        false
    }
}
