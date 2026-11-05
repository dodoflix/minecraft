//! `HoglinAi`.

use super::hoglin::Hoglin;
use mc_game_core::server::level::ServerLevel;
use mc_game_ai::world::entity::ai::behavior::animal_make_love::AnimalMakeLove;
use mc_game_ai::world::entity::ai::behavior::behaviors::{become_passive_if_memory_present, erase_memory_if};
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_ai::world::entity::ai::behavior::{baby_follow_adult, melee_attack, random_stroll, set_entity_look_target_sometimes, set_walk_target_from_attack_target_if_target_out_of_reach, start_attacking, stop_attacking_if_target_invalid};
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, BehaviorControl, DoNothing, GateBehavior, RunOne, set_walk_target_away_from, set_walk_target_from_look_target};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::EntityRef;
use mc_game_core::world::entity::ai::memory::memory_module_type::*;
use mc_game_core::world::entity::ai::sensing::sensor::is_entity_attackable;
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, SoundEvent};

pub const REPELLENT_DETECTION_RANGE_HORIZONTAL: i32 = 8;
pub const REPELLENT_DETECTION_RANGE_VERTICAL: i32 = 4;
/// `RETREAT_DURATION = TimeUtil.rangeOfSeconds(5, 20)`.
const RETREAT_DURATION: (i32, i32) = (5 * 20, 20 * 20);
const ATTACK_DURATION: i64 = 200;
const DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING: i32 = 8;
const DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING: i32 = 15;
const ATTACK_INTERVAL: i32 = 40;
const BABY_ATTACK_INTERVAL: i32 = 15;
const REPELLENT_PACIFY_TIME: i32 = 200;
/// `ADULT_FOLLOW_RANGE = UniformInt.of(5, 16)`.
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
const SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_RETREATING: f32 = 1.3;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 0.6;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.4;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 0.6;

fn b(behavior: impl BehaviorControl) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

/// `getActivities()`.
pub fn get_activities(_hoglin: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity(), init_fight_activity(), init_retreat_activity()]
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(Activity::CORE, 0, vec![b(LookAtTargetSink::new(45, 90)), b(MoveToTargetSink::new())])
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        10,
        vec![
            b(become_passive_if_memory_present(NEAREST_REPELLENT.id(), REPELLENT_PACIFY_TIME)),
            b(AnimalMakeLove::with(EntityType::HOGLIN, SPEED_MULTIPLIER_WHEN_MAKING_LOVE, 2)),
            b(set_walk_target_away_from::pos(NEAREST_REPELLENT, SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT, 8, true)),
            b(start_attacking::create(find_nearest_valid_attack_target)),
            b(BehaviorBuilder::trigger_if(is_adult, set_walk_target_away_from::entity(NEAREST_VISIBLE_ADULT_PIGLIN, SPEED_MULTIPLIER_WHEN_IDLING, DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING, false))),
            b(set_entity_look_target_sometimes::create(8.0, 30, 60)),
            b(baby_follow_adult::create(ADULT_FOLLOW_RANGE.0, ADULT_FOLLOW_RANGE.1, SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT)),
            b(create_idle_movement_behaviors()),
        ],
    )
}

fn init_fight_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::FIGHT,
        10,
        vec![
            b(become_passive_if_memory_present(NEAREST_REPELLENT.id(), REPELLENT_PACIFY_TIME)),
            b(AnimalMakeLove::with(EntityType::HOGLIN, SPEED_MULTIPLIER_WHEN_MAKING_LOVE, 2)),
            b(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
            b(BehaviorBuilder::trigger_if(is_adult, melee_attack::create(ATTACK_INTERVAL))),
            b(BehaviorBuilder::trigger_if(|e| e.is_baby(), melee_attack::create(BABY_ATTACK_INTERVAL))),
            b(stop_attacking_if_target_invalid::create()),
            b(erase_memory_if(is_breeding, ATTACK_TARGET.id())),
        ],
        ATTACK_TARGET.id(),
    )
}

fn init_retreat_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::AVOID,
        10,
        vec![
            b(set_walk_target_away_from::entity(AVOID_TARGET, SPEED_MULTIPLIER_WHEN_RETREATING, DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING, false)),
            b(create_idle_movement_behaviors()),
            b(set_entity_look_target_sometimes::create(8.0, 30, 60)),
            b(erase_memory_if(wants_to_stop_fleeing, AVOID_TARGET.id())),
        ],
        AVOID_TARGET.id(),
    )
}

fn create_idle_movement_behaviors() -> GateBehavior {
    RunOne::new(vec![
        (b(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)), 2),
        (b(set_walk_target_from_look_target::create(SPEED_MULTIPLIER_WHEN_IDLING, 3)), 2),
        (b(DoNothing::new(30, 60)), 1),
    ])
}

/// `updateActivity(body)`.
pub fn update_activity(level: &mut ServerLevel, body: &mut Hoglin) {
    let old_activity = body.brain().get_active_non_core_activity();
    body.brain_mut().set_active_activity_to_first_valid(&[Activity::FIGHT, Activity::AVOID, Activity::IDLE]);
    let new_activity = body.brain().get_active_non_core_activity();
    if old_activity != new_activity {
        if let Some(sound) = get_sound_for_current_activity(level, body) {
            make_sound(body, level, sound);
        }
    }
    let attacking = body.brain().has_memory_value(ATTACK_TARGET);
    body.mob.set_aggressive(attacking);
}

/// `onHitTarget(attackerBody, target)`.
pub fn on_hit_target(level: &mut ServerLevel, attacker_body: &mut Hoglin, target: &dyn Entity) {
    if !attacker_body.is_baby() {
        if target.get_type() == EntityType::PIGLIN && piglins_outnumber_hoglins(attacker_body) {
            set_avoid_target(level, attacker_body, target.id());
            broadcast_retreat(level, attacker_body, target);
        } else {
            broadcast_attack_target(level, attacker_body, target);
        }
    }
}

/// `BehaviorUtils.getNearestTarget(body, current, new)` over `(id, position)` pairs.
fn get_nearest_target(body: &dyn Entity, current: Option<(EntityId, Vec3)>, new: (EntityId, Vec3)) -> (EntityId, Vec3) {
    match current {
        Some(c) if body.base().distance_to_sqr_vec(c.1) < body.base().distance_to_sqr_vec(new.1) => c,
        _ => new,
    }
}

fn resolve(level: &ServerLevel, memory: Option<EntityRef>, hint: &dyn Entity) -> Option<(EntityId, Vec3)> {
    let EntityRef(id) = memory?;
    if id == hint.id() { Some((id, hint.position())) } else { level.get_entity(id).map(|e| (id, e.position())) }
}

fn broadcast_retreat(level: &mut ServerLevel, body: &Hoglin, target: &dyn Entity) {
    for EntityRef(id) in get_visible_adult_hoglins(body) {
        level.with_entity(id, |level, hoglin| retreat_from_nearest_target(level, hoglin, target));
    }
}

/// `retreatFromNearestTarget(body, newAvoidTarget)`.
fn retreat_from_nearest_target(level: &mut ServerLevel, body: &mut dyn Entity, new_avoid_target: &dyn Entity) {
    let avoid = resolve(level, body.brain().get_memory(&AVOID_TARGET).copied(), new_avoid_target);
    let mut nearest = get_nearest_target(body, avoid, (new_avoid_target.id(), new_avoid_target.position()));
    let attack = resolve(level, body.brain().get_memory(&ATTACK_TARGET).copied(), new_avoid_target);
    nearest = get_nearest_target(body, attack, nearest);
    set_avoid_target(level, body, nearest.0);
}

/// `setAvoidTarget(body, avoidTarget)`.
fn set_avoid_target(level: &mut ServerLevel, body: &mut dyn Entity, avoid_target: EntityId) {
    let duration = level.tick_state.random.next_int_between_inclusive(RETREAT_DURATION.0, RETREAT_DURATION.1);
    let brain = body.brain_mut();
    brain.erase_memory(&ATTACK_TARGET);
    brain.erase_memory(&WALK_TARGET);
    brain.set_memory_with_expiry(&AVOID_TARGET, EntityRef(avoid_target), i64::from(duration));
}

/// `findNearestValidAttackTarget(level, body)`.
fn find_nearest_valid_attack_target(_level: &ServerLevel, body: &dyn Entity) -> Option<EntityId> {
    if !is_pacified(body) && !is_breeding(body) { body.brain().get_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYER).map(|r| r.0) } else { None }
}

/// `isPosNearNearestRepellent(body, pos)`.
pub fn is_pos_near_nearest_repellent(body: &dyn Entity, pos: BlockPos) -> bool {
    body.brain().get_memory(&NEAREST_REPELLENT).is_some_and(|repellent| repellent.dist_sqr(&pos) < 8.0 * 8.0)
}

fn wants_to_stop_fleeing(body: &dyn Entity) -> bool {
    !body.is_baby() && !piglins_outnumber_hoglins(body)
}

/// `piglinsOutnumberHoglins(body)`.
fn piglins_outnumber_hoglins(body: &dyn Entity) -> bool {
    if body.is_baby() {
        return false;
    }
    let piglin_count = body.brain().get_memory(&VISIBLE_ADULT_PIGLIN_COUNT).copied().unwrap_or(0);
    let hoglin_count = body.brain().get_memory(&VISIBLE_ADULT_HOGLIN_COUNT).copied().unwrap_or(0) + 1;
    piglin_count > hoglin_count
}

/// `wasHurtBy(level, body, attacker)`.
pub fn was_hurt_by(level: &mut ServerLevel, body: &mut Hoglin, attacker: &dyn Entity) {
    body.brain_mut().erase_memory(&PACIFIED);
    body.brain_mut().erase_memory(&BREED_TARGET);
    if body.is_baby() {
        retreat_from_nearest_target(level, body, attacker);
    } else {
        maybe_retaliate(level, body, attacker);
    }
}

fn maybe_retaliate(level: &mut ServerLevel, body: &mut Hoglin, attacker: &dyn Entity) {
    if (!body.brain().is_active(Activity::AVOID) || attacker.get_type() != EntityType::PIGLIN)
        && attacker.get_type() != EntityType::HOGLIN
        && !mc_game_core::world::entity::ai::behavior::behavior_utils::is_other_target_much_further_away_than_current_attack_target(level, body, attacker, 4.0)
        && is_entity_attackable(level, body, attacker)
    {
        set_attack_target(body, attacker.id());
        broadcast_attack_target(level, body, attacker);
    }
}

/// `setAttackTarget(body, target)`.
fn set_attack_target(body: &mut dyn Entity, target: EntityId) {
    let brain = body.brain_mut();
    brain.erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
    brain.erase_memory(&BREED_TARGET);
    brain.set_memory_with_expiry(&ATTACK_TARGET, EntityRef(target), ATTACK_DURATION);
}

fn broadcast_attack_target(level: &mut ServerLevel, body: &Hoglin, target: &dyn Entity) {
    for EntityRef(id) in get_visible_adult_hoglins(body) {
        level.with_entity(id, |level, hoglin| set_attack_target_if_closer_than_current(level, hoglin, target));
    }
}

/// `setAttackTargetIfCloserThanCurrent(body, newTarget)`.
fn set_attack_target_if_closer_than_current(level: &mut ServerLevel, body: &mut dyn Entity, new_target: &dyn Entity) {
    if !is_pacified(body) {
        let current = resolve(level, body.brain().get_memory(&ATTACK_TARGET).copied(), new_target);
        let nearest = get_nearest_target(body, current, (new_target.id(), new_target.position()));
        set_attack_target(body, nearest.0);
    }
}

/// `getSoundForCurrentActivity(body)`.
pub fn get_sound_for_current_activity(level: &ServerLevel, body: &Hoglin) -> Option<SoundEvent> {
    body.brain().get_active_non_core_activity().map(|activity| get_sound_for_activity(level, body, activity))
}

fn get_sound_for_activity(level: &ServerLevel, body: &Hoglin, activity: Activity) -> SoundEvent {
    if activity == Activity::AVOID || body.is_converting(level) {
        SoundEvent::ENTITY_HOGLIN_RETREAT
    } else if activity == Activity::FIGHT {
        SoundEvent::ENTITY_HOGLIN_ANGRY
    } else if is_near_repellent(body) {
        SoundEvent::ENTITY_HOGLIN_RETREAT
    } else {
        SoundEvent::ENTITY_HOGLIN_AMBIENT
    }
}

fn get_visible_adult_hoglins(body: &dyn Entity) -> Vec<EntityRef> {
    body.brain().get_memory(&NEAREST_VISIBLE_ADULT_HOGLINS).cloned().unwrap_or_default()
}

fn is_near_repellent(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(NEAREST_REPELLENT)
}

fn is_breeding(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(BREED_TARGET)
}

/// `isPacified(body)`.
pub fn is_pacified(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(PACIFIED)
}

fn is_adult(e: &dyn Entity) -> bool {
    !e.is_baby()
}
