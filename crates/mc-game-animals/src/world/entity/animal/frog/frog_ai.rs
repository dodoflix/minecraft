//! `net.minecraft.world.entity.animal.frog.FrogAi`: the brain of the frog.

use super::shoot_tongue::ShootTongue;
use mc_game_core::server::level::ServerLevel;
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_ai::world::entity::ai::behavior::{animal_make_love::AnimalMakeLove, animal_panic::AnimalPanic, croak::Croak, follow_temptation::FollowTemptation, long_jump_mid_jump::LongJumpMidJump, long_jump_to_random_pos::{self, LongJumpToRandomPos}, random_stroll, set_entity_look_target_sometimes, start_attacking, stop_attacking_if_target_invalid, try_find_land, try_find_land_near_liquid, try_lay_spawn_on_fluid_near_land};
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, BehaviorControl, GateBehavior, OrderPolicy, RunOne, RunningPolicy, behavior_utils, move_to_target_sink::MoveToTargetSink, set_walk_target_from_look_target};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, BREED_TARGET, IS_IN_WATER, IS_PREGNANT, LONG_JUMP_COOLDOWN_TICKS, LONG_JUMP_MID_JUMP, NEAREST_ATTACKABLE, TEMPTATION_COOLDOWN_TICKS, TEMPTING_PLAYER, WALK_TARGET,
};
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::ai::navigation::get_path_type_static_for_mob as path_type_static_for_mob;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::pathfinder::PathType;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{EntityType, SoundEvent};

/// `TIME_BETWEEN_LONG_JUMPS` (`UniformInt.of(100, 140)`).
const TIME_BETWEEN_LONG_JUMPS: (i32, i32) = (100, 140);
/// `MAX_LONG_JUMP_HEIGHT`.
const MAX_LONG_JUMP_HEIGHT: i32 = 2;
/// `MAX_LONG_JUMP_WIDTH`.
const MAX_LONG_JUMP_WIDTH: i32 = 4;
/// `MAX_JUMP_VELOCITY_MULTIPLIER`.
const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 3.5714288;

/// `FrogAi.initMemories(body, random)`.
pub fn init_memories(body: &mut dyn Entity, random: &mut dyn RandomSource) {
    let cooldown = random.next_int_between_inclusive(TIME_BETWEEN_LONG_JUMPS.0, TIME_BETWEEN_LONG_JUMPS.1);
    body.brain_mut().set_memory(&LONG_JUMP_COOLDOWN_TICKS, cooldown);
}

/// `FrogAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity(), init_swim_activity(), init_lay_spawn_activity(), init_tongue_activity(), init_jump_activity()]
}

/// `StartAttacking.create((level, body) -> canAttack(body), (level, body) -> body.getBrain().getMemory(NEAREST_ATTACKABLE))`.
fn start_attacking_behavior() -> Box<dyn BehaviorControl> {
    boxed(start_attacking::create_with(|_, body| can_attack(body), |_, body| body.brain().get_memory(&NEAREST_ATTACKABLE).map(|r| r.0)))
}

/// `SetEntityLookTargetSometimes.create(EntityTypes.PLAYER, 6.0F, UniformInt.of(30, 60))`.
fn look_at_player_sometimes() -> Box<dyn BehaviorControl> {
    boxed(set_entity_look_target_sometimes::create_type(EntityType::PLAYER, 6.0, 30, 60))
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(AnimalPanic::new(2.0)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(LONG_JUMP_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let run_one: Box<dyn BehaviorControl> = boxed(RunOne::with_entry_condition(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![
            (boxed(random_stroll::stroll(1.0)), 1),
            (boxed(set_walk_target_from_look_target::create(1.0, 3)), 1),
            (boxed(Croak::new()), 3),
            (boxed(BehaviorBuilder::trigger_if_alone(|e| e.base().on_ground())), 2),
        ],
    ));
    ActivityData::create_pairs_with_conditions(
        Activity::IDLE,
        vec![
            (0, look_at_player_sometimes()),
            (0, boxed(AnimalMakeLove::new(EntityType::FROG))),
            (1, boxed(FollowTemptation::new(|_| 1.25))),
            (2, start_attacking_behavior()),
            (3, boxed(try_find_land::create(6, 1.0))),
            (4, run_one),
        ],
        vec![(LONG_JUMP_MID_JUMP.id(), MemoryStatus::ValueAbsent), (IS_IN_WATER.id(), MemoryStatus::ValueAbsent)],
    )
}

/// `initSwimActivity()`.
fn init_swim_activity() -> ActivityData {
    let gate: Box<dyn BehaviorControl> = boxed(GateBehavior::new(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![],
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (boxed(random_stroll::swim(0.75)), 1),
            (boxed(random_stroll::stroll_may_leave_water(1.0, true)), 1),
            (boxed(set_walk_target_from_look_target::create(1.0, 3)), 1),
            (boxed(BehaviorBuilder::trigger_if_alone(|e| e.is_in_water())), 5),
        ],
    ));
    ActivityData::create_pairs_with_conditions(
        Activity::SWIM,
        vec![
            (0, look_at_player_sometimes()),
            (1, boxed(FollowTemptation::new(|_| 1.25))),
            (2, start_attacking_behavior()),
            (3, boxed(try_find_land::create(8, 1.5))),
            (5, gate),
        ],
        vec![(LONG_JUMP_MID_JUMP.id(), MemoryStatus::ValueAbsent), (IS_IN_WATER.id(), MemoryStatus::ValuePresent)],
    )
}

/// `initLaySpawnActivity()`.
fn init_lay_spawn_activity() -> ActivityData {
    let run_one: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (boxed(random_stroll::stroll(1.0)), 2),
        (boxed(set_walk_target_from_look_target::create(1.0, 3)), 1),
        (boxed(Croak::new()), 2),
        (boxed(BehaviorBuilder::trigger_if_alone(|e| e.base().on_ground())), 1),
    ]));
    ActivityData::create_pairs_with_conditions(
        Activity::LAY_SPAWN,
        vec![
            (0, look_at_player_sometimes()),
            (1, start_attacking_behavior()),
            (2, boxed(try_find_land_near_liquid::create(8, 1.0, "frog_tries_to_find_land_near"))),
            (3, boxed(try_lay_spawn_on_fluid_near_land::create(Block::FROGSPAWN))),
            (4, run_one),
        ],
        vec![(LONG_JUMP_MID_JUMP.id(), MemoryStatus::ValueAbsent), (IS_PREGNANT.id(), MemoryStatus::ValuePresent)],
    )
}

/// `initJumpActivity()`.
fn init_jump_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::LONG_JUMP,
        vec![
            (0, boxed(LongJumpMidJump::new(TIME_BETWEEN_LONG_JUMPS.0, TIME_BETWEEN_LONG_JUMPS.1, SoundEvent::ENTITY_FROG_STEP))),
            (
                1,
                boxed(LongJumpToRandomPos::preferred_block(
                    TIME_BETWEEN_LONG_JUMPS,
                    MAX_LONG_JUMP_HEIGHT,
                    MAX_LONG_JUMP_WIDTH,
                    MAX_JUMP_VELOCITY_MULTIPLIER,
                    |_| SoundEvent::ENTITY_FROG_LONG_JUMP,
                    "frog_prefer_jump_to",
                    0.5,
                    is_acceptable_landing_spot,
                )),
            ),
        ],
        vec![
            (TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (LONG_JUMP_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
            (IS_IN_WATER.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// `initTongueActivity()`.
fn init_tongue_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::TONGUE,
        0,
        vec![
            boxed(stop_attacking_if_target_invalid::create()),
            boxed(ShootTongue::new(SoundEvent::ENTITY_FROG_TONGUE, SoundEvent::ENTITY_FROG_EAT)),
        ],
        ATTACK_TARGET.id(),
    )
}

/// `isAcceptableLandingSpot(body, targetPos)`.
fn is_acceptable_landing_spot(body: &mut dyn Entity, level: &ServerLevel, target_pos: BlockPos) -> bool {
    let below = target_pos.below();
    if level.get_fluid_state(target_pos).is_empty() && level.get_fluid_state(below).is_empty() && level.get_fluid_state(target_pos.above()).is_empty() {
        let bs = level.get_block_state(target_pos);
        let bs_below = level.get_block_state(below);
        if !bs.is_tag("frog_prefer_jump_to") && !bs_below.is_tag("frog_prefer_jump_to") {
            let path_type = path_type_static_for_mob(body, level, target_pos);
            let path_type_below = path_type_static_for_mob(body, level, below);
            if path_type != PathType::Trapdoor && (!bs.is_air() || path_type_below != PathType::Trapdoor) {
                long_jump_to_random_pos::default_acceptable_landing_spot(body, level, target_pos)
            } else {
                true
            }
        } else {
            true
        }
    } else {
        false
    }
}

/// `canAttack(mob)`.
fn can_attack(mob: &dyn Entity) -> bool {
    !behavior_utils::is_breeding(mob)
}

/// `FrogAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut().set_active_activity_to_first_valid(&[Activity::TONGUE, Activity::LAY_SPAWN, Activity::LONG_JUMP, Activity::SWIM, Activity::IDLE]);
}
