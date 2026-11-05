//! `Shoot`.

use super::breeze::Breeze;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::Unit;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, BREEZE_JUMP_TARGET, BREEZE_SHOOT, BREEZE_SHOOT_CHARGING, BREEZE_SHOOT_COOLDOWN, BREEZE_SHOOT_RECOVERING, WALK_TARGET,
};
use mc_game_core::world::entity::projectile::hurting::windcharge::BreezeWindCharge;
use mc_game_core::world::entity::projectile::spawn::spawn_projectile_using_shoot;
use mc_game_core::world::item::ItemStack;
use mc_data::SoundEvent;
use mc_data::enums::Pose;

/// `SHOOT_INITIAL_DELAY_TICKS = Math.round(15.0F)`.
const SHOOT_INITIAL_DELAY_TICKS: i32 = 15;
/// `SHOOT_RECOVER_DELAY_TICKS = Math.round(4.0F)`.
const SHOOT_RECOVER_DELAY_TICKS: i32 = 4;
/// `SHOOT_COOLDOWN_TICKS = Math.round(10.0F)`.
const SHOOT_COOLDOWN_TICKS: i32 = 10;

/// `Shoot extends Behavior<Breeze>`.
pub struct Shoot;

impl Shoot {
    pub fn new() -> BehaviorBase<Shoot> {
        BehaviorBase::with_timeout(
            entry_condition![
                ATTACK_TARGET => ValuePresent,
                BREEZE_SHOOT_COOLDOWN => ValueAbsent,
                BREEZE_SHOOT_CHARGING => ValueAbsent,
                BREEZE_SHOOT_RECOVERING => ValueAbsent,
                BREEZE_SHOOT => ValuePresent,
                WALK_TARGET => ValueAbsent,
                BREEZE_JUMP_TARGET => ValueAbsent
            ],
            SHOOT_INITIAL_DELAY_TICKS + 1 + SHOOT_RECOVER_DELAY_TICKS,
            Shoot,
        )
    }
}

impl Behavior for Shoot {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity) -> bool {
        if breeze.base().get_pose() != Pose::Standing {
            return false;
        }
        let Some(target) = breeze.brain().get_memory(&ATTACK_TARGET).map(|r| r.0) else { return false };
        let within_range = level.get_entity(target).map(|t| breeze.position().distance_to_sqr(t.position()) < 256.0);
        match within_range {
            Some(false) => {
                breeze.brain_mut().erase_memory(&BREEZE_SHOOT);
                false
            }
            Some(true) => true,
            None => false,
        }
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        body.brain().has_memory_value(ATTACK_TARGET) && body.brain().has_memory_value(BREEZE_SHOOT)
    }

    fn start(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        if breeze.brain().has_memory_value(ATTACK_TARGET) {
            breeze.set_pose(Pose::Shooting);
        }
        breeze.brain_mut().set_memory_with_expiry(&BREEZE_SHOOT_CHARGING, Unit, i64::from(SHOOT_INITIAL_DELAY_TICKS));
        breeze.play_sound(level, SoundEvent::ENTITY_BREEZE_INHALE, 1.0, 1.0);
    }

    fn stop(&mut self, _level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        if breeze.base().get_pose() == Pose::Shooting {
            breeze.set_pose(Pose::Standing);
        }
        breeze.brain_mut().set_memory_with_expiry(&BREEZE_SHOOT_COOLDOWN, Unit, i64::from(SHOOT_COOLDOWN_TICKS));
        breeze.brain_mut().erase_memory(&BREEZE_SHOOT);
    }

    fn tick(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = breeze.brain().get_memory(&ATTACK_TARGET).map(|r| r.0) else { return };
        let Some(target_position) = level.get_entity(target).map(|t| t.position()) else { return };
        breeze.look_at(mc_game_core::commands::arguments::entity_anchor_argument::Anchor::Eyes, target_position);
        if !breeze.brain().has_memory_value(BREEZE_SHOOT_CHARGING) && !breeze.brain().has_memory_value(BREEZE_SHOOT_RECOVERING) {
            breeze.brain_mut().set_memory_with_expiry(&BREEZE_SHOOT_RECOVERING, Unit, i64::from(SHOOT_RECOVER_DELAY_TICKS));
            let (xd, yd, zd) = {
                let t = level.get_entity(target).expect("resolved above");
                let y_scale = if t.base().is_passenger() { 0.8 } else { 0.3 };
                let target_y = t.base().y() + f64::from(t.base().dimensions.height) * y_scale;
                let firing_y = breeze.as_any().downcast_ref::<Breeze>().expect("breeze").get_firing_y_position();
                (t.base().x() - breeze.base().x(), target_y - firing_y, t.base().z() - breeze.base().z())
            };
            let firing_y = breeze.as_any().downcast_ref::<Breeze>().expect("breeze").get_firing_y_position();
            let charge = BreezeWindCharge::from_breeze(level, &*breeze, firing_y);
            let uncertainty = (5 - level.difficulty().get_id() * 4) as f32;
            spawn_projectile_using_shoot(level, Box::new(charge), &ItemStack::empty(), xd, yd, zd, 0.7, uncertainty);
            breeze.play_sound(level, SoundEvent::ENTITY_BREEZE_SHOOT, 1.5, 1.0);
        }
    }
}
