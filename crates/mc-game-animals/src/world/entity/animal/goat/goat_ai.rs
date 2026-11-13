//! `net.minecraft.world.entity.animal.goat.GoatAi`: the brain of the goat.

use super::goat::Goat;
use mc_core::random::RandomSource;
use mc_data::{EntityType, SoundEvent};
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::{
    animal_make_love::AnimalMakeLove, animal_panic::AnimalPanic, baby_follow_adult,
    follow_temptation::FollowTemptation, long_jump_mid_jump::LongJumpMidJump,
    long_jump_to_random_pos::LongJumpToRandomPos,
    prepare_ram_nearest_target::PrepareRamNearestTarget, ram_target::RamTarget, random_stroll,
    set_entity_look_target_sometimes,
};
use mc_game_core::world::entity::ai::behavior::do_nothing::DoNothing;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorControl, RunOne, move_to_target_sink::MoveToTargetSink,
    set_walk_target_from_look_target, swim::Swim,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    BREED_TARGET, LONG_JUMP_COOLDOWN_TICKS, RAM_COOLDOWN_TICKS, TEMPTATION_COOLDOWN_TICKS,
    TEMPTING_PLAYER, WALK_TARGET,
};
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;
use std::sync::Arc;

/// `RAM_PREPARE_TIME`.
pub const RAM_PREPARE_TIME: i32 = 20;
/// `RAM_MAX_DISTANCE`.
pub const RAM_MAX_DISTANCE: i32 = 7;
/// `ADULT_FOLLOW_RANGE` (`UniformInt.of(5, 16)`).
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM: f32 = 1.25;
/// `TIME_BETWEEN_LONG_JUMPS` (`UniformInt.of(600, 1200)`).
const TIME_BETWEEN_LONG_JUMPS: (i32, i32) = (600, 1200);
/// `MAX_LONG_JUMP_HEIGHT`.
pub const MAX_LONG_JUMP_HEIGHT: i32 = 5;
/// `MAX_LONG_JUMP_WIDTH`.
pub const MAX_LONG_JUMP_WIDTH: i32 = 5;
/// `MAX_JUMP_VELOCITY_MULTIPLIER`.
pub const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;
/// `TIME_BETWEEN_RAMS` (`UniformInt.of(600, 6000)`).
const TIME_BETWEEN_RAMS: (i32, i32) = (600, 6000);
/// `TIME_BETWEEN_RAMS_SCREAMER` (`UniformInt.of(100, 300)`).
const TIME_BETWEEN_RAMS_SCREAMER: (i32, i32) = (100, 300);
const SPEED_MULTIPLIER_WHEN_RAMMING: f32 = 3.0;
/// `RAM_MIN_DISTANCE`.
pub const RAM_MIN_DISTANCE: i32 = 4;
/// `ADULT_RAM_KNOCKBACK_FORCE`.
pub const ADULT_RAM_KNOCKBACK_FORCE: f64 = 2.5;
/// `BABY_RAM_KNOCKBACK_FORCE`.
pub const BABY_RAM_KNOCKBACK_FORCE: f64 = 1.0;

/// `RAM_TARGET_CONDITIONS`.
fn ram_target_conditions() -> TargetingConditions {
    TargetingConditions::for_combat().selector(Some(Arc::new(|target, level| {
        target.get_type() != EntityType::GOAT
            && (level.mob_griefing() || target.get_type() != EntityType::ARMOR_STAND)
            && level
                .world_border()
                .is_within_bounds_aabb(&target.base().bounding_box())
    })))
}

/// The goat behind a behavior's `body`.
fn goat(body: &dyn Entity) -> &Goat {
    body.as_any().downcast_ref::<Goat>().expect("goat")
}

/// `goat.isScreamingGoat() ? TIME_BETWEEN_RAMS_SCREAMER : TIME_BETWEEN_RAMS`.
fn time_between_rams(body: &dyn Entity) -> (i32, i32) {
    if goat(body).is_screaming_goat() {
        TIME_BETWEEN_RAMS_SCREAMER
    } else {
        TIME_BETWEEN_RAMS
    }
}

/// Picks the screaming variant of a goat sound.
fn sound_of(body: &dyn Entity, regular: SoundEvent, screaming: SoundEvent) -> SoundEvent {
    if goat(body).is_screaming_goat() {
        screaming
    } else {
        regular
    }
}

/// `GoatAi.initMemories(body, random)`.
pub fn init_memories(body: &mut dyn Entity, random: &mut dyn RandomSource) {
    let long_jump =
        random.next_int_between_inclusive(TIME_BETWEEN_LONG_JUMPS.0, TIME_BETWEEN_LONG_JUMPS.1);
    body.brain_mut()
        .set_memory(&LONG_JUMP_COOLDOWN_TICKS, long_jump);
    let ram = random.next_int_between_inclusive(TIME_BETWEEN_RAMS.0, TIME_BETWEEN_RAMS.1);
    body.brain_mut().set_memory(&RAM_COOLDOWN_TICKS, ram);
}

/// `GoatAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_long_jump_activity(),
        init_ram_activity(),
    ]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(LONG_JUMP_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(RAM_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let run_one: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (
            boxed(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
            2,
        ),
        (
            boxed(set_walk_target_from_look_target::create(
                SPEED_MULTIPLIER_WHEN_IDLING,
                3,
            )),
            2,
        ),
        (boxed(DoNothing::new(30, 60)), 1),
    ]));
    ActivityData::create_pairs_with_conditions(
        Activity::IDLE,
        vec![
            (
                0,
                boxed(set_entity_look_target_sometimes::create_type(
                    EntityType::PLAYER,
                    6.0,
                    30,
                    60,
                )),
            ),
            (0, boxed(AnimalMakeLove::new(EntityType::GOAT))),
            (
                1,
                boxed(FollowTemptation::new(|_| SPEED_MULTIPLIER_WHEN_TEMPTED)),
            ),
            (
                2,
                boxed(baby_follow_adult::create(
                    ADULT_FOLLOW_RANGE.0,
                    ADULT_FOLLOW_RANGE.1,
                    SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                )),
            ),
            (3, run_one),
        ],
        vec![
            (RAM_COOLDOWN_TICKS.id(), MemoryStatus::ValuePresent),
            (LONG_JUMP_COOLDOWN_TICKS.id(), MemoryStatus::ValuePresent),
        ],
    )
}

/// `initLongJumpActivity()`.
fn init_long_jump_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::LONG_JUMP,
        vec![
            (
                0,
                boxed(LongJumpMidJump::new(
                    TIME_BETWEEN_LONG_JUMPS.0,
                    TIME_BETWEEN_LONG_JUMPS.1,
                    SoundEvent::ENTITY_GOAT_STEP,
                )),
            ),
            (
                1,
                boxed(LongJumpToRandomPos::new(
                    TIME_BETWEEN_LONG_JUMPS,
                    MAX_LONG_JUMP_HEIGHT,
                    MAX_LONG_JUMP_WIDTH,
                    MAX_JUMP_VELOCITY_MULTIPLIER,
                    |body| {
                        sound_of(
                            body,
                            SoundEvent::ENTITY_GOAT_LONG_JUMP,
                            SoundEvent::ENTITY_GOAT_SCREAMING_LONG_JUMP,
                        )
                    },
                )),
            ),
        ],
        vec![
            (TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (LONG_JUMP_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// `initRamActivity()`.
fn init_ram_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::RAM,
        vec![
            (
                0,
                boxed(RamTarget::new(
                    time_between_rams,
                    ram_target_conditions(),
                    SPEED_MULTIPLIER_WHEN_RAMMING,
                    |body| {
                        if body.is_baby() {
                            BABY_RAM_KNOCKBACK_FORCE
                        } else {
                            ADULT_RAM_KNOCKBACK_FORCE
                        }
                    },
                    |body| {
                        sound_of(
                            body,
                            SoundEvent::ENTITY_GOAT_RAM_IMPACT,
                            SoundEvent::ENTITY_GOAT_SCREAMING_RAM_IMPACT,
                        )
                    },
                    |_| SoundEvent::ENTITY_GOAT_HORN_BREAK,
                    |level, body| {
                        body.as_any_mut()
                            .downcast_mut::<Goat>()
                            .expect("goat")
                            .drop_horn(level)
                    },
                )),
            ),
            (
                1,
                boxed(PrepareRamNearestTarget::new(
                    |body| time_between_rams(body).0,
                    RAM_MIN_DISTANCE,
                    RAM_MAX_DISTANCE,
                    SPEED_MULTIPLIER_WHEN_PREPARING_TO_RAM,
                    ram_target_conditions(),
                    RAM_PREPARE_TIME,
                    |body| {
                        sound_of(
                            body,
                            SoundEvent::ENTITY_GOAT_PREPARE_RAM,
                            SoundEvent::ENTITY_GOAT_SCREAMING_PREPARE_RAM,
                        )
                    },
                )),
            ),
        ],
        vec![
            (TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (RAM_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// `GoatAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut().set_active_activity_to_first_valid(&[
        Activity::RAM,
        Activity::LONG_JUMP,
        Activity::IDLE,
    ]);
}
