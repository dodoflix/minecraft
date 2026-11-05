//! `SonicBoom`.

use super::warden;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::Unit;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_COOLING_DOWN, ATTACK_TARGET, SONIC_BOOM_COOLDOWN, SONIC_BOOM_SOUND_COOLDOWN, SONIC_BOOM_SOUND_DELAY,
};
use mc_game_core::world::entity::dimensions::EntityAttachment;
use crate::world::entity::monster::warden::closer_than_xz_y;
use mc_core::math::mth;
use mc_data::{Attribute, ParticleType, SoundEvent};
use mc_protocol::packets::play::ParticleOptions;

const DISTANCE_XZ: f64 = 15.0;
const DISTANCE_Y: f64 = 20.0;
const KNOCKBACK_VERTICAL: f64 = 0.5;
const KNOCKBACK_HORIZONTAL: f64 = 2.5;
/// `COOLDOWN`.
pub const COOLDOWN: i32 = 40;
/// `TICKS_BEFORE_PLAYING_SOUND = Mth.ceil(34.0)`.
const TICKS_BEFORE_PLAYING_SOUND: i64 = 34;
/// `DURATION = Mth.ceil(60.0F)`.
const DURATION: i32 = 60;

/// `SonicBoom extends Behavior<Warden>`.
pub struct SonicBoom;

impl SonicBoom {
    /// `new SonicBoom()`.
    pub fn new() -> BehaviorBase<SonicBoom> {
        BehaviorBase::with_timeout(
            entry_condition![
                ATTACK_TARGET => ValuePresent,
                SONIC_BOOM_COOLDOWN => ValueAbsent,
                SONIC_BOOM_SOUND_COOLDOWN => Registered,
                SONIC_BOOM_SOUND_DELAY => Registered
            ],
            DURATION,
            SonicBoom,
        )
    }

    /// `setCooldown(body, cooldown)`.
    pub fn set_cooldown(body: &mut dyn Entity, cooldown: i32) {
        body.brain_mut().set_memory_with_expiry(&SONIC_BOOM_COOLDOWN, Unit, i64::from(cooldown));
    }
}

impl Behavior for SonicBoom {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        let target = body.brain().get_memory(&ATTACK_TARGET).expect("attack target present").0;
        level.get_entity(target).is_some_and(|t| closer_than_xz_y(body, t, DISTANCE_XZ, DISTANCE_Y))
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) -> bool {
        true
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().set_memory_with_expiry(&ATTACK_COOLING_DOWN, true, i64::from(DURATION));
        body.brain_mut().set_memory_with_expiry(&SONIC_BOOM_SOUND_DELAY, Unit, TICKS_BEFORE_PLAYING_SOUND);
        level.broadcast_entity_event(body.id(), 62);
        body.play_sound(level, SoundEvent::ENTITY_WARDEN_SONIC_CHARGE, 3.0, 1.0);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(target) = body.brain().get_memory(&ATTACK_TARGET).map(|t| t.0)
            && let Some(target_position) = level.get_entity(target).map(|t| t.position())
        {
            mc_game_core::world::entity::mob::look_at_xyz(body, target_position.x, target_position.y, target_position.z);
        }
        if !body.brain().has_memory_value(SONIC_BOOM_SOUND_DELAY) && !body.brain().has_memory_value(SONIC_BOOM_SOUND_COOLDOWN) {
            body.brain_mut().set_memory_with_expiry(&SONIC_BOOM_SOUND_COOLDOWN, Unit, i64::from(DURATION) - TICKS_BEFORE_PLAYING_SOUND);
            let target = body
                .brain()
                .get_memory(&ATTACK_TARGET)
                .map(|t| t.0)
                .filter(|id| level.get_entity(*id).is_some_and(|t| warden(body).can_target_entity_ref(level, t) && closer_than_xz_y(body, t, DISTANCE_XZ, DISTANCE_Y)));
            let Some(target) = target else { return };
            let (eye_position, knockback_resistance) = {
                let t = level.get_entity(target).expect("checked above");
                (t.base().get_eye_position(), t.living().map_or(0.0, |l| l.get_attribute_value(Attribute::KNOCKBACK_RESISTANCE)))
            };
            let source = body.position().add(body.get_attachments().get(EntityAttachment::WardenChest, 0, body.base().y_rot()));
            let delta = eye_position.subtract(source);
            let normalize = delta.normalize();
            let steps = mth::floor_f64(delta.length()) + 7;
            for i in 1..steps {
                let particle_pos = source.add(normalize.scale(f64::from(i)));
                level.send_particles(ParticleOptions::Simple(ParticleType::SONIC_BOOM), particle_pos.x, particle_pos.y, particle_pos.z, 1, 0.0, 0.0, 0.0, 0.0);
            }
            body.play_sound(level, SoundEvent::ENTITY_WARDEN_SONIC_BOOM, 3.0, 1.0);
            let damage_source = level.damage_sources().sonic_boom(&*body);
            let hurt = level.with_entity(target, |level, t| t.hurt_server(level, &damage_source, 10.0)).unwrap_or(false);
            if hurt {
                let knockback_vertical = KNOCKBACK_VERTICAL * (1.0 - knockback_resistance);
                let knockback_horizontal = KNOCKBACK_HORIZONTAL * (1.0 - knockback_resistance);
                level.with_entity(target, |_, t| t.push_xyz(normalize.x * knockback_horizontal, normalize.y * knockback_vertical, normalize.z * knockback_horizontal));
            }
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        Self::set_cooldown(body, COOLDOWN);
    }
}
