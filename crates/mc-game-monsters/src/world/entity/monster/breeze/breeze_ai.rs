//! `BreezeAi`.

use super::long_jump::LongJump;
use super::shoot::Shoot;
use super::shoot_when_stuck::ShootWhenStuck;
use super::slide::Slide;
use mc_data::SoundEvent;
use mc_data::enums::Pose;
use mc_game_ai::world::entity::ai::behavior::behaviors::boxed;
use mc_game_ai::world::entity::ai::behavior::{
    random_stroll, start_attacking, stop_attacking_if_target_invalid,
};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::swim::Swim;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase, DoNothing, RunOne};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, BREEZE_SHOOT, HURT_BY, NEAREST_ATTACKABLE, WALK_TARGET,
};
use mc_game_core::world::entity::ai::memory::{MemoryStatus, Unit};
use mc_game_core::world::entity::ai::sensing::sensor::is_entity_attackable;
use mc_game_core::world::entity::schedule::Activity;

/// `SPEED_MULTIPLIER_WHEN_SLIDING`.
pub const SPEED_MULTIPLIER_WHEN_SLIDING: f32 = 0.6;
/// `JUMP_CIRCLE_INNER_RADIUS` / `MIDDLE` / `OUTER`.
pub const JUMP_CIRCLE_INNER_RADIUS: f32 = 4.0;
pub const JUMP_CIRCLE_MIDDLE_RADIUS: f32 = 8.0;
pub const JUMP_CIRCLE_OUTER_RADIUS: f32 = 24.0;
const TICKS_TO_REMEMBER_SEEN_TARGET: i32 = 100;

/// `getActivities(breeze)`.
pub fn get_activities(breeze: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_fight_activity(breeze),
    ]
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![boxed(Swim::new(0.8)), boxed(LookAtTargetSink::new(45, 90))],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (
                0,
                boxed(start_attacking::create(
                    |_level: &ServerLevel, breeze: &dyn Entity| {
                        breeze.brain().get_memory(&NEAREST_ATTACKABLE).map(|r| r.0)
                    },
                )),
            ),
            (
                1,
                boxed(start_attacking::create(
                    |level: &ServerLevel, breeze: &dyn Entity| {
                        let source = breeze.brain().get_memory(&HURT_BY)?;
                        let entity = source.0.get_entity()?;
                        level
                            .get_entity(entity)
                            .filter(|e| e.living().is_some())
                            .map(|e| e.id())
                    },
                )),
            ),
            (2, boxed(slide_to_target_sink(20, 40))),
            (
                3,
                boxed(RunOne::new(vec![
                    (boxed(DoNothing::new(20, 100)), 1),
                    (boxed(random_stroll::stroll(0.6)), 2),
                ])),
            ),
        ],
    )
}

fn init_fight_activity(_body: &dyn Entity) -> ActivityData {
    let mut positives_left = 0i32;
    ActivityData::create_pairs_with_conditions(
        Activity::FIGHT,
        vec![
            (
                0,
                boxed(stop_attacking_if_target_invalid::create_when(
                    move |level: &ServerLevel, body: &dyn Entity, target: &dyn Entity| {
                        // `Sensor.wasEntityAttackableLastNTicks(body, 100).negate()` (`rememberPositives`)
                        let attackable = is_entity_attackable(level, body, target);
                        let remembered = if attackable {
                            positives_left = TICKS_TO_REMEMBER_SEEN_TARGET;
                            true
                        } else {
                            positives_left -= 1;
                            positives_left >= 0
                        };
                        !remembered
                    },
                )),
            ),
            (1, boxed(Shoot::new())),
            (2, boxed(LongJump::new())),
            (3, boxed(ShootWhenStuck::new())),
            (4, boxed(Slide::new())),
        ],
        vec![
            (ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (WALK_TARGET.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// `updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::FIGHT, Activity::IDLE]);
}

/// `BreezeAi.SlideToTargetSink extends MoveToTargetSink`.
struct SlideToTargetSink(MoveToTargetSink);

fn slide_to_target_sink(min_timeout: i32, max_timeout: i32) -> BehaviorBase<SlideToTargetSink> {
    let base = MoveToTargetSink::with_timeout(min_timeout, max_timeout);
    let entry_condition = base.entry_condition.clone();
    BehaviorBase::with_duration(
        entry_condition,
        min_timeout,
        max_timeout,
        SlideToTargetSink(base.inner),
    )
}

impl Behavior for SlideToTargetSink {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        self.0.check_extra_start_conditions(level, body)
    }
    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        self.0.can_still_use(level, body, timestamp)
    }
    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.tick(level, body, timestamp);
    }
    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.0.timed_out(timestamp, end_timestamp)
    }
    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.start(level, body, timestamp);
        body.play_sound(level, SoundEvent::ENTITY_BREEZE_SLIDE, 1.0, 1.0);
        body.set_pose(Pose::Sliding);
    }
    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.stop(level, body, timestamp);
        body.set_pose(Pose::Standing);
        if body.brain().has_memory_value(ATTACK_TARGET) {
            body.brain_mut()
                .set_memory_with_expiry(&BREEZE_SHOOT, Unit, 60);
        }
    }
}
