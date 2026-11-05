//! `ShootWhenStuck`.

use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::Unit;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, BREEZE_JUMP_INHALING, BREEZE_JUMP_TARGET, BREEZE_SHOOT, WALK_TARGET};
use mc_data::MobEffect;

/// `ShootWhenStuck extends Behavior<Breeze>`.
pub struct ShootWhenStuck;

impl ShootWhenStuck {
    pub fn new() -> BehaviorBase<ShootWhenStuck> {
        BehaviorBase::new(
            entry_condition![ATTACK_TARGET => ValuePresent, BREEZE_JUMP_INHALING => ValueAbsent, BREEZE_JUMP_TARGET => ValueAbsent, WALK_TARGET => ValueAbsent, BREEZE_SHOOT => ValueAbsent],
            ShootWhenStuck,
        )
    }
}

impl Behavior for ShootWhenStuck {
    fn check_extra_start_conditions(&mut self, _level: &mut ServerLevel, breeze: &mut dyn Entity) -> bool {
        breeze.base().is_passenger() || breeze.is_in_water() || breeze.get_effect(MobEffect::LEVITATION).is_some()
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) -> bool {
        false
    }

    fn start(&mut self, _level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        breeze.brain_mut().set_memory_with_expiry(&BREEZE_SHOOT, Unit, 60);
    }
}
