//! `net.minecraft.world.level.block.FrogspawnBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::hatch_spawns;
use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_data::{EntityType, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct FrogspawnBlock;

fn may_place_on(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let fluid_state = level.get_fluid_state(pos);
    let fluid_above = level.get_fluid_state(pos.above());
    (fluid_state.is_tag("supports_frogspawn") || level.get_block_state(pos).is_tag("supports_frogspawn")) && fluid_above.is_empty()
}

const DEFAULT_MIN_HATCH_TICK_DELAY: i32 = 3600;
const DEFAULT_MAX_HATCH_TICK_DELAY: i32 = 12000;

/// `destroyBlock(level, pos)`.
pub fn destroy_block(level: &mut ServerLevel, pos: BlockPos) {
    level.destroy_block(pos, false, None, 512);
}

/// `getRandomTadpolePositionOffset`.
fn get_random_tadpole_position_offset(random: &mut dyn RandomSource) -> f64 {
    mth::clamp_f64(random.next_double(), f64::from(0.2f32), 0.7999999970197678)
}

/// `spawnTadpoles`.
fn spawn_tadpoles(level: &mut ServerLevel, pos: BlockPos) {
    let tadpole_amount = 2 + level.random().next_int_bounded(4);
    for _ in 1..=tadpole_amount {
        let x_pos = f64::from(pos.x()) + get_random_tadpole_position_offset(level.random());
        let z_pos = f64::from(pos.z()) + get_random_tadpole_position_offset(level.random());
        let y_rot = 1 + level.random().next_int_bounded(360);
        let mut tag = hatch_spawns::snap_to_tag(x_pos, f64::from(pos.y()) - 0.5, z_pos, y_rot as f32, 0.0);
        tag.put_boolean("PersistenceRequired", true);
        hatch_spawns::spawn_breeding(level, EntityType::TADPOLE, &tag);
    }
}

impl BlockBehaviour for FrogspawnBlock {
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if entity.get_type() == EntityType::FALLING_BLOCK {
            destroy_block(level, pos);
        }
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        let delay = DEFAULT_MIN_HATCH_TICK_DELAY + level.random().next_int_bounded(DEFAULT_MAX_HATCH_TICK_DELAY - DEFAULT_MIN_HATCH_TICK_DELAY);
        helpers::schedule_block_tick(level, pos, state.block(), delay);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !self.can_survive(state, &*level, pos) {
            destroy_block(level, pos);
        } else {
            destroy_block(level, pos);
            level.play_sound(None, pos, SoundEvent::BLOCK_FROGSPAWN_HATCH, SoundSource::Blocks, 1.0, 1.0);
            spawn_tadpoles(level, pos);
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        may_place_on(level, pos.below())
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !self.can_survive(state, &*level, pos) { air() } else { state }
    });
}

register_block_class!("FrogspawnBlock", |_| Box::new(FrogspawnBlock));
