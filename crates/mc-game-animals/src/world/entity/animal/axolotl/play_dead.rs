//! `net.minecraft.world.entity.animal.axolotl.PlayDead`.

use mc_data::MobEffect;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    HURT_BY_ENTITY, LOOK_TARGET, PLAY_DEAD_TICKS, WALK_TARGET,
};
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::entity::Entity;

/// `PlayDead`: lies still (regenerating) while the `PLAY_DEAD_TICKS` memory counts down.
pub struct PlayDead;

impl PlayDead {
    /// `new PlayDead()`.
    pub fn new() -> BehaviorBase<PlayDead> {
        BehaviorBase::with_timeout(
            entry_condition![PLAY_DEAD_TICKS => ValuePresent, HURT_BY_ENTITY => ValuePresent],
            200,
            PlayDead,
        )
    }
}

impl Behavior for PlayDead {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.is_in_water()
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.is_in_water() && body.brain().has_memory_value(PLAY_DEAD_TICKS.id())
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let brain = body.brain_mut();
        brain.erase_memory(&WALK_TARGET);
        brain.erase_memory(&LOOK_TARGET);
        body.add_effect(
            level,
            MobEffectInstance::of(MobEffect::REGENERATION, 200, 0),
            None,
        );
    }
}
