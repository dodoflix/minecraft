//! `PiglinBruteAi`.

use super::abstract_piglin::AbstractPiglin as _;
use super::piglin_ai;
use super::piglin_brute::PiglinBrute;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_ai::world::entity::ai::behavior::{melee_attack, random_stroll, set_walk_target_from_attack_target_if_target_out_of_reach, start_attacking, stop_attacking_if_target_invalid, stop_being_angry_if_target_dead};
use mc_game_core::world::entity::ai::behavior::{BehaviorControl, DoNothing, GateBehavior, RunOne, interact_with, interact_with_door, set_entity_look_target, set_look_and_interact, stroll_around_poi, stroll_to_poi};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ANGRY_AT, ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE, HOME, INTERACTION_TARGET, NEAREST_VISIBLE_ATTACKABLE_PLAYER, NEAREST_VISIBLE_NEMESIS};
use mc_game_core::world::entity::ai::sensing::sensor::is_entity_attackable_ignoring_line_of_sight;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_protocol::codec::GlobalPos;

const ANGER_DURATION: i64 = 600;
const MELEE_ATTACK_COOLDOWN: i32 = 20;
const ACTIVITY_SOUND_LIKELIHOOD_PER_TICK: f32 = 0.0125;
const HOME_CLOSE_ENOUGH_DISTANCE: i32 = 2;
const HOME_TOO_FAR_DISTANCE: i32 = 100;
const HOME_STROLL_AROUND_DISTANCE: i32 = 5;

fn b(behavior: impl BehaviorControl) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

/// `getActivities(piglin)`.
pub fn get_activities(_piglin: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity(), init_fight_activity()]
}

/// `initMemories(body)`: `HOME` = the spawn position.
pub fn init_memories(body: &mut dyn Entity, dimension: mc_core::ident::Identifier) {
    let current_global_pos = GlobalPos { dimension, pos: body.block_position() };
    body.brain_mut().set_memory(&HOME, current_global_pos);
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![b(LookAtTargetSink::new(45, 90)), b(MoveToTargetSink::new()), b(interact_with_door::create()), b(stop_being_angry_if_target_dead::create())],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        10,
        vec![
            b(start_attacking::create(find_nearest_valid_attack_target)),
            b(create_idle_look_behaviors()),
            b(create_idle_movement_behaviors()),
            b(set_look_and_interact::create(EntityType::PLAYER, 4)),
        ],
    )
}

fn init_fight_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::FIGHT,
        10,
        vec![
            b(stop_attacking_if_target_invalid::create_when(|level, body, target| !is_nearest_valid_attack_target(level, body, target))),
            b(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
            b(melee_attack::create(MELEE_ATTACK_COOLDOWN)),
        ],
        ATTACK_TARGET.id(),
    )
}

fn create_idle_look_behaviors() -> GateBehavior {
    RunOne::new(vec![
        (b(set_entity_look_target::create_type(EntityType::PLAYER, 8.0)), 1),
        (b(set_entity_look_target::create_type(EntityType::PIGLIN, 8.0)), 1),
        (b(set_entity_look_target::create_type(EntityType::PIGLIN_BRUTE, 8.0)), 1),
        (b(set_entity_look_target::create(8.0)), 1),
        (b(DoNothing::new(30, 60)), 1),
    ])
}

fn create_idle_movement_behaviors() -> GateBehavior {
    RunOne::new(vec![
        (b(random_stroll::stroll(0.6)), 2),
        (b(interact_with::of(EntityType::PIGLIN, 8, INTERACTION_TARGET, 0.6, 2)), 2),
        (b(interact_with::of(EntityType::PIGLIN_BRUTE, 8, INTERACTION_TARGET, 0.6, 2)), 2),
        (b(stroll_to_poi::create(HOME, 0.6, HOME_CLOSE_ENOUGH_DISTANCE, HOME_TOO_FAR_DISTANCE)), 2),
        (b(stroll_around_poi::create(HOME, 0.6, HOME_STROLL_AROUND_DISTANCE)), 2),
        (b(DoNothing::new(30, 60)), 1),
    ])
}

/// `updateActivity(body)`.
pub fn update_activity(level: &mut ServerLevel, body: &mut PiglinBrute) {
    let old_activity = body.brain().get_active_non_core_activity();
    body.brain_mut().set_active_activity_to_first_valid(&[Activity::FIGHT, Activity::IDLE]);
    let new_activity = body.brain().get_active_non_core_activity();
    if old_activity != new_activity {
        play_activity_sound(level, body);
    }
    let attacking = body.brain().has_memory_value(ATTACK_TARGET);
    body.mob.set_aggressive(attacking);
}

fn is_nearest_valid_attack_target(level: &ServerLevel, body: &dyn Entity, target: &dyn Entity) -> bool {
    find_nearest_valid_attack_target(level, body) == Some(target.id())
}

/// `findNearestValidAttackTarget(level, body)`.
fn find_nearest_valid_attack_target(level: &ServerLevel, body: &dyn Entity) -> Option<EntityId> {
    let angry_at = mc_game_core::world::entity::ai::behavior::behavior_utils::get_living_entity_from_uuid_memory(level, body, &ANGRY_AT);
    if let Some(id) = angry_at {
        if level.get_entity(id).is_some_and(|t| is_entity_attackable_ignoring_line_of_sight(level, body, t)) {
            return Some(id);
        }
    }
    let player = body.brain().get_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYER);
    if player.is_some() { player.map(|r| r.0) } else { body.brain().get_memory(&NEAREST_VISIBLE_NEMESIS).map(|r| r.0) }
}

/// `wasHurtBy(level, body, attacker)`.
pub fn was_hurt_by(level: &mut ServerLevel, body: &mut PiglinBrute, attacker: &dyn Entity) {
    if !matches!(attacker.get_type(), EntityType::PIGLIN | EntityType::PIGLIN_BRUTE) {
        piglin_ai::maybe_retaliate(level, body, attacker);
    }
}

/// `setAngerTarget(body, target)`.
pub fn set_anger_target(body: &mut PiglinBrute, target: &dyn Entity) {
    let brain = body.brain_mut();
    brain.erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
    brain.set_memory_with_expiry(&ANGRY_AT, target.uuid(), ANGER_DURATION);
}

/// `maybePlayActivitySound(body)`.
pub fn maybe_play_activity_sound(level: &mut ServerLevel, body: &mut PiglinBrute) {
    if level.tick_state.random.next_float() < ACTIVITY_SOUND_LIKELIHOOD_PER_TICK {
        play_activity_sound(level, body);
    }
}

fn play_activity_sound(level: &mut ServerLevel, body: &mut PiglinBrute) {
    if body.brain().get_active_non_core_activity() == Some(Activity::FIGHT) {
        body.play_angry_sound(level);
    }
}
