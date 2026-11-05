//! `BehaviorUtils`: helpers shared by many behaviors (the ones that need only the brain framework; item throwing,
//! swimmable positions, attack ranges and village sections come with the behaviors that use them).

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::ai::behavior::position_tracker::{BlockPosTracker, EntityTracker, PositionTracker};
use crate::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, BREED_TARGET, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET};
use crate::world::entity::ai::memory::{EntityRef, MemoryModuleType, WalkTarget};
use crate::world::entity::entity::Entity;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use uuid::Uuid;

/// `entityIsVisible(brain, targetEntity)`.
pub fn entity_is_visible(level: &ServerLevel, body: &dyn Entity, target: EntityId) -> bool {
    body.brain().get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES).is_some_and(|visible| visible.contains(level, body, target))
}

/// `targetIsValid(brain, memory, targetType)` / the predicate form: the remembered entity matches, is alive and visible.
pub fn target_is_valid(level: &ServerLevel, body: &dyn Entity, memory: &MemoryModuleType<EntityRef>, target_predicate: impl Fn(&dyn Entity) -> bool) -> bool {
    body.brain().get_memory(memory).is_some_and(|&EntityRef(id)| {
        level.get_entity(id).is_some_and(|e| target_predicate(e) && e.is_alive()) && entity_is_visible(level, body, id)
    })
}

/// `lookAtEntity(looker, targetEntity)`: `LOOK_TARGET = new EntityTracker(target, true)`.
pub fn look_at_entity(looker: &mut dyn Entity, target: &dyn Entity) {
    looker.brain_mut().set_memory(&LOOK_TARGET, PositionTracker::Entity(EntityTracker::new(target, true)));
}

/// `lockGazeAndWalkToEachOther(entity1, entity2, speedModifier, closeEnoughDistance)`.
pub fn lock_gaze_and_walk_to_each_other(entity1: &mut dyn Entity, entity2: &mut dyn Entity, speed_modifier: f32, close_enough_distance: i32) {
    look_at_entity(entity1, &*entity2);
    look_at_entity(entity2, &*entity1);
    set_walk_and_look_target_memories_entity(entity1, &*entity2, speed_modifier, close_enough_distance);
    set_walk_and_look_target_memories_entity(entity2, &*entity1, speed_modifier, close_enough_distance);
}

/// `setWalkAndLookTargetMemories(walker, targetEntity, speedModifier, closeEnoughDistance)`.
pub fn set_walk_and_look_target_memories_entity(walker: &mut dyn Entity, target: &dyn Entity, speed_modifier: f32, close_enough_distance: i32) {
    set_walk_and_look_target_memories(walker, PositionTracker::Entity(EntityTracker::new(target, true)), speed_modifier, close_enough_distance);
}

/// `setWalkAndLookTargetMemories(walker, targetPos, speedModifier, closeEnoughDistance)`.
pub fn set_walk_and_look_target_memories_pos(walker: &mut dyn Entity, target_pos: BlockPos, speed_modifier: f32, close_enough_distance: i32) {
    set_walk_and_look_target_memories(walker, PositionTracker::Block(BlockPosTracker::new(target_pos)), speed_modifier, close_enough_distance);
}

/// `setWalkAndLookTargetMemories(walker, target, speedModifier, closeEnoughDistance)`.
pub fn set_walk_and_look_target_memories(walker: &mut dyn Entity, target: PositionTracker, speed_modifier: f32, close_enough_distance: i32) {
    let walk_target = WalkTarget::new(target.clone(), speed_modifier, close_enough_distance);
    walker.brain_mut().set_memory(&LOOK_TARGET, target);
    walker.brain_mut().set_memory(&WALK_TARGET, walk_target);
}

/// `isOtherTargetMuchFurtherAwayThanCurrentAttackTarget(body, otherTarget, howMuchFurtherAway)`.
pub fn is_other_target_much_further_away_than_current_attack_target(level: &ServerLevel, body: &dyn Entity, other_target: &dyn Entity, how_much_further_away: f64) -> bool {
    let Some(&EntityRef(current)) = body.brain().get_memory(&ATTACK_TARGET) else { return false };
    let Some(current) = level.get_entity(current) else { return false };
    let dist_to_current = body.base().distance_to_sqr_vec(current.position());
    let dist_to_other = body.base().distance_to_sqr_vec(other_target.position());
    dist_to_other > dist_to_current + how_much_further_away * how_much_further_away
}

/// `canSee(body, target)`.
pub fn can_see(level: &ServerLevel, body: &dyn Entity, target: EntityId) -> bool {
    entity_is_visible(level, body, target)
}

/// `getTargetNearestMe(body, target1, target2)`: `target1` if strictly closer than `target2`, else `target2`.
pub fn get_target_nearest_me(body: &dyn Entity, target1: EntityId, pos1: Vec3, target2: EntityId, pos2: Vec3) -> EntityId {
    if body.base().distance_to_sqr_vec(pos1) < body.base().distance_to_sqr_vec(pos2) { target1 } else { target2 }
}

/// `getLivingEntityFromUUIDMemory(body, memoryType)`.
pub fn get_living_entity_from_uuid_memory(level: &ServerLevel, body: &dyn Entity, memory_type: &MemoryModuleType<Uuid>) -> Option<EntityId> {
    let uuid = *body.brain().get_memory(memory_type)?;
    level.get_entity_by_uuid(uuid).filter(|e| e.living().is_some()).map(|e| e.id())
}

/// `isBreeding(body)`.
pub fn is_breeding(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(BREED_TARGET)
}
