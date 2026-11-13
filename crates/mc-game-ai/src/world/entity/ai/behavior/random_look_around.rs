//! `RandomLookAround`.

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    GAZE_COOLDOWN_TICKS, LOOK_TARGET,
};
use mc_game_core::world::entity::entity::Entity;
use mc_worldgen::provider::int_provider::IntProvider;

pub struct RandomLookAround {
    interval: IntProvider,
    max_yaw: f32,
    min_pitch: f32,
    pitch_range: f32,
}

impl RandomLookAround {
    /// `new RandomLookAround(interval, maxYaw, minPitch, maxPitch)`.
    pub fn new(
        interval: IntProvider,
        max_yaw: f32,
        min_pitch: f32,
        max_pitch: f32,
    ) -> BehaviorBase<RandomLookAround> {
        assert!(
            min_pitch <= max_pitch,
            "Minimum pitch is larger than maximum pitch! {min_pitch} > {max_pitch}"
        );
        BehaviorBase::new(
            entry_condition![LOOK_TARGET => ValueAbsent, GAZE_COOLDOWN_TICKS => ValueAbsent],
            RandomLookAround {
                interval,
                max_yaw,
                min_pitch,
                pitch_range: max_pitch - min_pitch,
            },
        )
    }

    /// `new RandomLookAround(UniformInt.of(min, max), maxYaw, minPitch, maxPitch)`.
    pub fn new_uniform(
        min_inclusive: i32,
        max_inclusive: i32,
        max_yaw: f32,
        min_pitch: f32,
        max_pitch: f32,
    ) -> BehaviorBase<RandomLookAround> {
        Self::new(
            IntProvider::Uniform {
                min_inclusive,
                max_inclusive,
            },
            max_yaw,
            min_pitch,
            max_pitch,
        )
    }
}

impl Behavior for RandomLookAround {
    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let random = &mut body.base_mut().random;
        let pitch = mth::clamp_f32(
            random.next_float() * self.pitch_range + self.min_pitch,
            -90.0,
            90.0,
        );
        let y_offset = 2.0 * random.next_float() * self.max_yaw;
        let gaze_cooldown = self.interval.sample(random);
        let rotation = mth::wrap_degrees_f32(body.base().y_rot() + y_offset - self.max_yaw);
        let new_look_vec = Vec3::direction_from_rotation(pitch, rotation);
        let look_at = body.base().get_eye_position().add(new_look_vec);
        body.brain_mut().set_memory(
            &LOOK_TARGET,
            PositionTracker::Block(BlockPosTracker::from_vec3(look_at)),
        );
        body.brain_mut()
            .set_memory(&GAZE_COOLDOWN_TICKS, gaze_cooldown);
    }
}
