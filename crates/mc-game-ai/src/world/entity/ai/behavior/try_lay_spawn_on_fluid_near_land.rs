//! `TryLaySpawnOnFluidNearLand`.

use mc_core::direction::Direction;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_data::{GameEvent, SoundEvent};
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, IS_PREGNANT, WALK_TARGET,
};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::level_access::LevelAccessor;
use mc_protocol::packets::play::SoundSource;

/// `create(spawnBlock)`.
pub fn create(spawn_block: Block) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let _attack_target = i.absent(&ATTACK_TARGET);
    let _walk_target = i.present(&WALK_TARGET);
    let pregnant = i.present(&IS_PREGNANT);
    i.create(move |c| {
        if c.body.is_in_water() || !c.body.base().on_ground() {
            return false;
        }
        let below_pos = c.body.block_position().below();
        for direction in Direction::HORIZONTAL {
            let relative_pos = below_pos.relative(direction);
            let level = &*c.level;
            let state = level.get_block_state(relative_pos);
            let supports = state
                .get_collision_shape(level, relative_pos)
                .get_face_shape(Direction::Up)
                .is_empty()
                && (mc_data::tags::get::<Fluid>("supports_frogspawn")
                    .is_some_and(|t| t.contains(level.get_fluid_state(relative_pos).fluid()))
                    || state.is_tag("supports_frogspawn"));
            if supports {
                let spawn_pos = relative_pos.above();
                if level.get_block_state(spawn_pos).is_air() {
                    let new_state = spawn_block.default_state();
                    let body_id = c.body.id();
                    c.level.set_block_and_update(spawn_pos, new_state);
                    c.level.game_event_at_block(
                        GameEvent::BLOCK_PLACE,
                        spawn_pos,
                        &GameEventContext::of(Some(body_id), Some(new_state)),
                    );
                    behavior_utils::play_sound_from_entity(
                        &mut *c.level,
                        body_id,
                        SoundEvent::ENTITY_FROG_LAY_SPAWN,
                        SoundSource::Blocks,
                        1.0,
                        1.0,
                    );
                    c.erase(&pregnant);
                    return true;
                }
            }
        }
        true
    })
}
