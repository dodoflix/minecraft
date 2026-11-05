//! `LongJumpMidJump`.

use super::behavior_utils;
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{LONG_JUMP_COOLDOWN_TICKS, LONG_JUMP_MID_JUMP, LOOK_TARGET};
use crate::world::entity::entity::Entity;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::SoundEvent;
use mc_data::enums::Pose;
use mc_protocol::packets::play::SoundSource;

/// `TIME_OUT_DURATION`.
pub const TIME_OUT_DURATION: i32 = 100;

pub struct LongJumpMidJump {
    time_between_long_jumps: (i32, i32),
    landing_sound: SoundEvent,
}

impl LongJumpMidJump {
    /// `new LongJumpMidJump(timeBetweenLongJumps, landingSound)`; the `UniformInt` as `(min, max)` inclusive.
    pub fn new(time_between_long_jumps_min: i32, time_between_long_jumps_max: i32, landing_sound: SoundEvent) -> BehaviorBase<LongJumpMidJump> {
        BehaviorBase::with_timeout(
            entry_condition![LOOK_TARGET => Registered, LONG_JUMP_MID_JUMP => ValuePresent],
            TIME_OUT_DURATION,
            LongJumpMidJump { time_between_long_jumps: (time_between_long_jumps_min, time_between_long_jumps_max), landing_sound },
        )
    }
}

impl Behavior for LongJumpMidJump {
    fn can_still_use(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        !body.base().on_ground()
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.living_mut().expect("living").discard_friction = true;
        body.set_pose(Pose::LongJumping);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base().on_ground() {
            let dm = body.base().delta_movement();
            body.base_mut().set_delta_movement(dm.multiply_xyz(f64::from(0.1_f32), 1.0, f64::from(0.1_f32)));
            behavior_utils::play_sound_from_entity(level, body.id(), self.landing_sound, SoundSource::Neutral, 2.0, 1.0);
        }
        body.living_mut().expect("living").discard_friction = false;
        body.set_pose(Pose::Standing);
        body.brain_mut().erase_memory(&LONG_JUMP_MID_JUMP);
        let cooldown = level.tick_state.random.next_int_between_inclusive(self.time_between_long_jumps.0, self.time_between_long_jumps.1);
        body.brain_mut().set_memory(&LONG_JUMP_COOLDOWN_TICKS, cooldown);
    }
}
