//! `Roar`.

use super::warden_mut;
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::behavior::behavior_utils;
use crate::world::entity::ai::behavior::{Behavior, BehaviorBase};
use crate::world::entity::ai::memory::Unit;
use crate::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, ROAR_SOUND_COOLDOWN, ROAR_SOUND_DELAY, ROAR_TARGET, WALK_TARGET};
use crate::world::entity::monster::warden::warden_ai::ROAR_DURATION;
use mc_data::SoundEvent;
use mc_data::enums::Pose;

const TICKS_BEFORE_PLAYING_ROAR_SOUND: i64 = 25;
const ROAR_ANGER_INCREASE: i32 = 20;

/// `Roar extends Behavior<Warden>`.
pub struct Roar;

impl Roar {
    /// `new Roar()`.
    pub fn new() -> BehaviorBase<Roar> {
        BehaviorBase::with_timeout(
            entry_condition![ROAR_TARGET => ValuePresent, ATTACK_TARGET => ValueAbsent, ROAR_SOUND_COOLDOWN => Registered, ROAR_SOUND_DELAY => Registered],
            ROAR_DURATION,
            Roar,
        )
    }
}

impl Behavior for Roar {
    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().set_memory_with_expiry(&ROAR_SOUND_DELAY, Unit, TICKS_BEFORE_PLAYING_ROAR_SOUND);
        body.brain_mut().erase_memory(&WALK_TARGET);
        let target = body.brain().get_memory(&ROAR_TARGET).expect("roar target present").0;
        if let Some(target_entity) = level.get_entity(target) {
            behavior_utils::look_at_entity(body, target_entity);
        }
        body.set_pose(Pose::Roaring);
        warden_mut(body).increase_anger_at(level, Some(target), ROAR_ANGER_INCREASE, false);
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) -> bool {
        true
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if !body.brain().has_memory_value(ROAR_SOUND_DELAY) && !body.brain().has_memory_value(ROAR_SOUND_COOLDOWN) {
            body.brain_mut().set_memory_with_expiry(&ROAR_SOUND_COOLDOWN, Unit, i64::from(ROAR_DURATION) - TICKS_BEFORE_PLAYING_ROAR_SOUND);
            body.play_sound(level, SoundEvent::ENTITY_WARDEN_ROAR, 3.0, 1.0);
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base().get_pose() == Pose::Roaring {
            body.set_pose(Pose::Standing);
        }
        if let Some(target) = body.brain().get_memory(&ROAR_TARGET).map(|t| t.0) {
            warden_mut(body).set_attack_target(target);
        }
        body.brain_mut().erase_memory(&ROAR_TARGET);
    }
}
