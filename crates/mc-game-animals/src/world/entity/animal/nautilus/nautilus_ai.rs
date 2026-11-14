//! `net.minecraft.world.entity.animal.nautilus.NautilusAi`: the brain of the nautilus (also the attack target search of the
//! zombie nautilus).

use mc_core::random::RandomSource;
use mc_data::{EntityType, SoundEvent};
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::{
    animal_make_love::AnimalMakeLove, animal_panic::AnimalPanic, charge_attack::ChargeAttack,
    follow_temptation::FollowTemptation, random_stroll,
};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorBuilder, BehaviorControl, GateBehavior, OneShot, OrderPolicy, RunningPolicy,
    behavior_utils, move_to_target_sink::MoveToTargetSink, set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ANGRY_AT, ATTACK_TARGET, ATTACK_TARGET_COOLDOWN, BREED_TARGET, CANT_REACH_WALK_TARGET_SINCE,
    CHARGE_COOLDOWN_TICKS, NEAREST_VISIBLE_LIVING_ENTITIES, TEMPTATION_COOLDOWN_TICKS,
    TEMPTING_PLAYER, WALK_TARGET,
};
use mc_game_core::world::entity::ai::memory::{EntityRef, MemoryStatus};
use mc_game_core::world::entity::ai::sensing::sensor;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::tamable;
use mc_game_core::world::entity::schedule::Activity;
use std::sync::Arc;

const SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.3;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 0.4;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 1.6;
/// `TIME_BETWEEN_NON_PLAYER_ATTACKS` (`UniformInt.of(2400, 3600)`).
const TIME_BETWEEN_NON_PLAYER_ATTACKS: (i32, i32) = (2400, 3600);
const SPEED_WHEN_ATTACKING: f32 = 0.6;
const ATTACK_KNOCKBACK_FORCE: f32 = 2.0;
const TIME_BETWEEN_ATTACKS: i32 = 80;
const MAX_CHARGE_DISTANCE: f64 = 12.0;
const MAX_TARGET_DETECTION_DISTANCE: f64 = 11.0;

/// `ATTACK_TARGET_CONDITIONS`.
pub(super) fn attack_target_conditions() -> TargetingConditions {
    TargetingConditions::for_combat().selector(Some(Arc::new(|target, level| {
        (level.mob_griefing() || target.get_type() != EntityType::ARMOR_STAND)
            && level
                .world_border()
                .is_within_bounds_aabb(&target.base().bounding_box())
    })))
}

/// `NautilusAi.initMemories(body, random)`.
pub fn init_memories(body: &mut dyn Entity, random: &mut dyn RandomSource) {
    let cooldown = random.next_int_between_inclusive(
        TIME_BETWEEN_NON_PLAYER_ATTACKS.0,
        TIME_BETWEEN_NON_PLAYER_ATTACKS.1,
    );
    body.brain_mut()
        .set_memory(&ATTACK_TARGET_COOLDOWN, cooldown);
}

/// `NautilusAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_fight_activity(),
    ]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(CHARGE_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(ATTACK_TARGET_COOLDOWN)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let gate: Box<dyn BehaviorControl> = boxed(GateBehavior::new(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![],
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (
                boxed(random_stroll::swim(SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER)),
                2,
            ),
            (
                boxed(set_walk_target_from_look_target::create(
                    SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER,
                    3,
                )),
                3,
            ),
        ],
    ));
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (
                1,
                boxed(AnimalMakeLove::with(
                    EntityType::NAUTILUS,
                    SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                    2,
                )),
            ),
            (
                2,
                boxed(FollowTemptation::with_distance(
                    |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                    |mob| if mob.is_baby() { 2.5 } else { 3.5 },
                )),
            ),
            (3, boxed(start_attacking())),
            (4, gate),
        ],
    )
}

/// `initFightActivity()`.
fn init_fight_activity() -> ActivityData {
    fight_activity(SPEED_WHEN_ATTACKING, SoundEvent::ENTITY_NAUTILUS_DASH)
}

/// The fight activity of both brains: a `ChargeAttack` while there is a target, no tempting player, no breeding and no
/// charge cooldown.
pub(super) fn fight_activity(speed: f32, charge_sound: SoundEvent) -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::FIGHT,
        vec![(
            0,
            boxed(ChargeAttack::new(
                TIME_BETWEEN_ATTACKS,
                attack_target_conditions(),
                speed,
                ATTACK_KNOCKBACK_FORCE,
                MAX_CHARGE_DISTANCE,
                MAX_TARGET_DETECTION_DISTANCE,
                charge_sound,
            )),
        )],
        vec![
            (ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (TEMPTING_PLAYER.id(), MemoryStatus::ValueAbsent),
            (BREED_TARGET.id(), MemoryStatus::ValueAbsent),
            (CHARGE_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// `StartAttacking.create(NautilusAi::findNearestValidAttackTarget)`: the finder rolls the level random and writes a brain
/// memory, so it takes the level and the body mutably (the stock behavior's finder does not).
pub(super) fn start_attacking() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let attack_target = i.absent(&ATTACK_TARGET);
    let cant_reach_since = i.registered(&CANT_REACH_WALK_TARGET_SINCE);
    i.create(move |c| {
        let Some(target) = find_nearest_valid_attack_target(&mut *c.level, &mut *c.body) else {
            return false;
        };
        let can_attack = c
            .level
            .get_entity(target)
            .is_some_and(|t| c.body.can_attack(&*c.level, t));
        if !can_attack {
            return false;
        }
        c.set(&attack_target, EntityRef(target));
        c.erase(&cant_reach_since);
        true
    })
}

/// `NautilusAi.findNearestValidAttackTarget(level, body)`.
pub(super) fn find_nearest_valid_attack_target(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
) -> Option<EntityId> {
    if behavior_utils::is_breeding(body)
        || !body.is_in_water()
        || body.is_baby()
        || (body.mob().is_some_and(|m| m.tamable.is_some()) && tamable::is_tame(body))
    {
        return None;
    }
    let angry_at = behavior_utils::get_living_entity_from_uuid_memory(level, body, &ANGRY_AT)
        .filter(|&id| {
            level.get_entity(id).is_some_and(|e| {
                e.is_in_water()
                    && sensor::is_entity_attackable_ignoring_line_of_sight(level, body, e)
            })
        });
    if angry_at.is_some() {
        return angry_at;
    }
    if body.brain().has_memory_value(ATTACK_TARGET_COOLDOWN) {
        return None;
    }
    let random = &mut level.tick_state.random;
    let cooldown = random.next_int_between_inclusive(
        TIME_BETWEEN_NON_PLAYER_ATTACKS.0,
        TIME_BETWEEN_NON_PLAYER_ATTACKS.1,
    );
    body.brain_mut()
        .set_memory(&ATTACK_TARGET_COOLDOWN, cooldown);
    if random.next_float() < 0.5 {
        return None;
    }
    body.brain()
        .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)?
        .find_closest(level, body, is_hostile_target)
        .map(|r| r.0)
}

/// `isHostileTarget(mob)`.
fn is_hostile_target(mob: &dyn Entity) -> bool {
    mob.is_in_water()
        && mc_data::tags::get::<EntityType>("nautilus_hostiles")
            .is_some_and(|t| t.contains(mob.get_type()))
}

/// `NautilusAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::FIGHT, Activity::IDLE]);
}
