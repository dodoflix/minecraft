//! `net.minecraft.world.entity.animal.axolotl.AxolotlAi`: the brain of the axolotl.

use super::axolotl::on_stop_attacking;
use super::play_dead::PlayDead;
use super::validate_play_dead;
use mc_data::EntityType;
use mc_game_ai::world::entity::ai::behavior::behaviors::{
    CountDownCooldownTicks, boxed, erase_memory_if,
};
use mc_game_ai::world::entity::ai::behavior::{
    animal_make_love::AnimalMakeLove, baby_follow_adult, follow_temptation::FollowTemptation,
    melee_attack, random_stroll, set_entity_look_target_sometimes,
    set_walk_target_from_attack_target_if_target_out_of_reach, start_attacking,
    stop_attacking_if_target_invalid, try_find_liquid,
};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorBuilder, BehaviorControl, GateBehavior, OneShot, OrderPolicy, RunOne, RunningPolicy,
    behavior_utils, move_to_target_sink::MoveToTargetSink,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, HAS_HUNTING_COOLDOWN, LOOK_TARGET, NEAREST_ATTACKABLE, NEAREST_VISIBLE_ADULT,
    PLAY_DEAD_TICKS, TEMPTATION_COOLDOWN_TICKS, WALK_TARGET,
};
use mc_game_core::world::entity::ai::memory::{MemoryStatus, WalkTarget};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::level::biome_ext::is_water_at;

/// `ADULT_FOLLOW_RANGE` (`UniformInt.of(5, 16)`).
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);

/// `AxolotlAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_fight_activity(),
        init_play_dead_activity(),
    ]
}

/// `initPlayDeadActivity()`.
fn init_play_dead_activity() -> ActivityData {
    ActivityData::create_full(
        Activity::PLAY_DEAD,
        vec![
            (0, boxed(PlayDead::new())),
            (
                1,
                boxed(erase_memory_if(
                    behavior_utils::is_breeding,
                    PLAY_DEAD_TICKS.id(),
                )),
            ),
        ],
        vec![(PLAY_DEAD_TICKS.id(), MemoryStatus::ValuePresent)],
        vec![PLAY_DEAD_TICKS.id()],
    )
}

/// `initFightActivity()`.
fn init_fight_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::FIGHT,
        0,
        vec![
            boxed(stop_attacking_if_target_invalid::create_on_erased(
                |level: &mut ServerLevel, body: &mut dyn Entity, target: EntityId| {
                    on_stop_attacking(level, body, target)
                },
            )),
            boxed(
                set_walk_target_from_attack_target_if_target_out_of_reach::create_with(
                    get_speed_modifier_chasing,
                ),
            ),
            boxed(melee_attack::create(20)),
            boxed(erase_memory_if(
                behavior_utils::is_breeding,
                ATTACK_TARGET.id(),
            )),
        ],
        ATTACK_TARGET.id(),
    )
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(validate_play_dead::create()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let follow: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (boxed(FollowTemptation::new(get_speed_modifier)), 1),
        (
            boxed(baby_follow_adult::create_with(
                ADULT_FOLLOW_RANGE.0,
                ADULT_FOLLOW_RANGE.1,
                get_speed_modifier_following_adult,
                NEAREST_VISIBLE_ADULT,
                false,
            )),
            1,
        ),
    ]));
    let gate: Box<dyn BehaviorControl> = boxed(GateBehavior::new(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![],
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (boxed(random_stroll::swim(0.5)), 2),
            (boxed(random_stroll::stroll_may_leave_water(0.15, false)), 2),
            (boxed(set_walk_target_from_look_target_in_same_medium()), 3),
            (
                boxed(BehaviorBuilder::trigger_if_alone(|e| e.is_in_water())),
                5,
            ),
            (
                boxed(BehaviorBuilder::trigger_if_alone(|e| e.base().on_ground())),
                5,
            ),
        ],
    ));
    ActivityData::create_pairs(
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
            (1, boxed(AnimalMakeLove::with(EntityType::AXOLOTL, 0.2, 2))),
            (2, follow),
            (
                3,
                boxed(start_attacking::create(|_, body| {
                    find_nearest_valid_attack_target(body)
                })),
            ),
            (
                3,
                boxed(try_find_liquid::create(6, 0.15, "axolotl_tries_to_find")),
            ),
            (4, gate),
        ],
    )
}

/// `SetWalkTargetFromLookTarget.create(AxolotlAi::canSetWalkTargetFromLookTarget, AxolotlAi::getSpeedModifier, 3)`: the
/// predicate needs the level (`level.isWaterAt(pos) == body.isInWater()`), which the stock behavior's predicate does not get.
fn set_walk_target_from_look_target_in_same_medium() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    let look_target = i.present(&LOOK_TARGET);
    i.create(move |c| {
        let tracker = c.get(&look_target).clone();
        if !can_set_walk_target_from_look_target(&*c.level, c.body()) {
            return false;
        }
        let speed = get_speed_modifier(c.body());
        c.set(&walk_target, WalkTarget::new(tracker, speed, 3));
        true
    })
}

/// `canSetWalkTargetFromLookTarget(body)`.
fn can_set_walk_target_from_look_target(level: &ServerLevel, body: &dyn Entity) -> bool {
    match body.brain().get_memory(&LOOK_TARGET) {
        Some(look_target) => {
            is_water_at(level, look_target.current_block_position(level)) == body.is_in_water()
        }
        None => false,
    }
}

/// `AxolotlAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    let old_activity = body.brain().get_active_non_core_activity();
    if old_activity != Some(Activity::PLAY_DEAD) {
        body.brain_mut().set_active_activity_to_first_valid(&[
            Activity::PLAY_DEAD,
            Activity::FIGHT,
            Activity::IDLE,
        ]);
        if old_activity == Some(Activity::FIGHT)
            && body.brain().get_active_non_core_activity() != Some(Activity::FIGHT)
        {
            body.brain_mut()
                .set_memory_with_expiry(&HAS_HUNTING_COOLDOWN, true, 2400);
        }
    }
}

/// `getSpeedModifierChasing(mob)`.
fn get_speed_modifier_chasing(mob: &dyn Entity) -> f32 {
    if mob.is_in_water() { 0.6 } else { 0.15 }
}

/// `getSpeedModifierFollowingAdult(mob)`.
fn get_speed_modifier_following_adult(mob: &dyn Entity) -> f32 {
    if mob.is_in_water() { 0.6 } else { 0.15 }
}

/// `getSpeedModifier(mob)`.
fn get_speed_modifier(mob: &dyn Entity) -> f32 {
    if mob.is_in_water() { 0.5 } else { 0.15 }
}

/// `findNearestValidAttackTarget(level, body)`.
fn find_nearest_valid_attack_target(body: &dyn Entity) -> Option<EntityId> {
    if behavior_utils::is_breeding(body) {
        None
    } else {
        body.brain().get_memory(&NEAREST_ATTACKABLE).map(|r| r.0)
    }
}
