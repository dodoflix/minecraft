//! `BehaviorUtils`: helpers shared by many behaviors (the ones that need only the brain framework; item throwing,
//! swimmable positions, attack ranges and village sections come with the behaviors that use them).

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, EntityTracker, PositionTracker,
};
use crate::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, BREED_TARGET, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET,
};
use crate::world::entity::ai::memory::{EntityRef, MemoryModuleType, WalkTarget};
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::entity::Entity;
use crate::world::entity::item::ItemEntity;
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::PathComputationType;
use uuid::Uuid;

/// `entityIsVisible(brain, targetEntity)`.
pub fn entity_is_visible(level: &ServerLevel, body: &dyn Entity, target: EntityId) -> bool {
    body.brain()
        .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
        .is_some_and(|visible| visible.contains(level, body, target))
}

/// `targetIsValid(brain, memory, targetType)` / the predicate form: the remembered entity matches, is alive and visible.
pub fn target_is_valid(
    level: &ServerLevel,
    body: &dyn Entity,
    memory: &MemoryModuleType<EntityRef>,
    target_predicate: impl Fn(&dyn Entity) -> bool,
) -> bool {
    body.brain()
        .get_memory(memory)
        .is_some_and(|&EntityRef(id)| {
            level
                .get_entity(id)
                .is_some_and(|e| target_predicate(e) && e.is_alive())
                && entity_is_visible(level, body, id)
        })
}

/// `lookAtEntity(looker, targetEntity)`: `LOOK_TARGET = new EntityTracker(target, true)`.
pub fn look_at_entity(looker: &mut dyn Entity, target: &dyn Entity) {
    looker.brain_mut().set_memory(
        &LOOK_TARGET,
        PositionTracker::Entity(EntityTracker::new(target, true)),
    );
}

/// `lockGazeAndWalkToEachOther(entity1, entity2, speedModifier, closeEnoughDistance)`.
pub fn lock_gaze_and_walk_to_each_other(
    entity1: &mut dyn Entity,
    entity2: &mut dyn Entity,
    speed_modifier: f32,
    close_enough_distance: i32,
) {
    look_at_entity(entity1, &*entity2);
    look_at_entity(entity2, &*entity1);
    set_walk_and_look_target_memories_entity(
        entity1,
        &*entity2,
        speed_modifier,
        close_enough_distance,
    );
    set_walk_and_look_target_memories_entity(
        entity2,
        &*entity1,
        speed_modifier,
        close_enough_distance,
    );
}

/// `setWalkAndLookTargetMemories(walker, targetEntity, speedModifier, closeEnoughDistance)`.
pub fn set_walk_and_look_target_memories_entity(
    walker: &mut dyn Entity,
    target: &dyn Entity,
    speed_modifier: f32,
    close_enough_distance: i32,
) {
    set_walk_and_look_target_memories(
        walker,
        PositionTracker::Entity(EntityTracker::new(target, true)),
        speed_modifier,
        close_enough_distance,
    );
}

/// `setWalkAndLookTargetMemories(walker, targetPos, speedModifier, closeEnoughDistance)`.
pub fn set_walk_and_look_target_memories_pos(
    walker: &mut dyn Entity,
    target_pos: BlockPos,
    speed_modifier: f32,
    close_enough_distance: i32,
) {
    set_walk_and_look_target_memories(
        walker,
        PositionTracker::Block(BlockPosTracker::new(target_pos)),
        speed_modifier,
        close_enough_distance,
    );
}

/// `setWalkAndLookTargetMemories(walker, target, speedModifier, closeEnoughDistance)`.
pub fn set_walk_and_look_target_memories(
    walker: &mut dyn Entity,
    target: PositionTracker,
    speed_modifier: f32,
    close_enough_distance: i32,
) {
    let walk_target = WalkTarget::new(target.clone(), speed_modifier, close_enough_distance);
    walker.brain_mut().set_memory(&LOOK_TARGET, target);
    walker.brain_mut().set_memory(&WALK_TARGET, walk_target);
}

/// `isOtherTargetMuchFurtherAwayThanCurrentAttackTarget(body, otherTarget, howMuchFurtherAway)`.
pub fn is_other_target_much_further_away_than_current_attack_target(
    level: &ServerLevel,
    body: &dyn Entity,
    other_target: &dyn Entity,
    how_much_further_away: f64,
) -> bool {
    let Some(&EntityRef(current)) = body.brain().get_memory(&ATTACK_TARGET) else {
        return false;
    };
    let Some(current) = level.get_entity(current) else {
        return false;
    };
    let dist_to_current = body.base().distance_to_sqr_vec(current.position());
    let dist_to_other = body.base().distance_to_sqr_vec(other_target.position());
    dist_to_other > dist_to_current + how_much_further_away * how_much_further_away
}

/// `canSee(body, target)`.
pub fn can_see(level: &ServerLevel, body: &dyn Entity, target: EntityId) -> bool {
    entity_is_visible(level, body, target)
}

/// `getTargetNearestMe(body, target1, target2)`: `target1` if strictly closer than `target2`, else `target2`.
pub fn get_target_nearest_me(
    body: &dyn Entity,
    target1: EntityId,
    pos1: Vec3,
    target2: EntityId,
    pos2: Vec3,
) -> EntityId {
    if body.base().distance_to_sqr_vec(pos1) < body.base().distance_to_sqr_vec(pos2) {
        target1
    } else {
        target2
    }
}

/// `getLivingEntityFromUUIDMemory(body, memoryType)`.
pub fn get_living_entity_from_uuid_memory(
    level: &ServerLevel,
    body: &dyn Entity,
    memory_type: &MemoryModuleType<Uuid>,
) -> Option<EntityId> {
    let uuid = *body.brain().get_memory(memory_type)?;
    level
        .get_entity_by_uuid(uuid)
        .filter(|e| e.living().is_some())
        .map(|e| e.id())
}

/// `isBreeding(body)`.
pub fn is_breeding(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(BREED_TARGET)
}

/// `targetIsValid(brain, memory, targetType)`.
pub fn target_is_valid_type(
    level: &ServerLevel,
    body: &dyn Entity,
    memory: &MemoryModuleType<EntityRef>,
    target_type: mc_data::EntityType,
) -> bool {
    target_is_valid(level, body, memory, |e| e.get_type() == target_type)
}

/// `throwItem(thrower, item, targetPos)`.
pub fn throw_item(
    level: &mut ServerLevel,
    thrower: &dyn Entity,
    item: ItemStack,
    target_pos: Vec3,
) {
    let throw_velocity = Vec3::new(f64::from(0.3_f32), f64::from(0.3_f32), f64::from(0.3_f32));
    throw_item_full(level, thrower, item, target_pos, throw_velocity, 0.3);
}

/// `throwItem(thrower, item, targetPos, throwVelocity, handYDistanceFromEye)`.
pub fn throw_item_full(
    level: &mut ServerLevel,
    thrower: &dyn Entity,
    item: ItemStack,
    target_pos: Vec3,
    throw_velocity: Vec3,
    hand_y_distance_from_eye: f32,
) {
    let y_hand_pos = thrower.base().get_eye_y() - f64::from(hand_y_distance_from_eye);
    let p = thrower.position();
    let mut item_entity = ItemEntity::with_stack(level, p.x, y_hand_pos, p.z, item);
    item_entity.set_thrower(thrower.uuid());
    let throw_vector = target_pos
        .subtract(thrower.position())
        .normalize()
        .multiply_xyz(throw_velocity.x, throw_velocity.y, throw_velocity.z);
    item_entity.base_mut().set_delta_movement(throw_vector);
    item_entity.set_default_pick_up_delay();
    level.add_fresh_entity(Box::new(item_entity));
}

/// `getNearestTarget(body, target1, target2)`: `target2` when there is no `target1` or either does not resolve.
pub fn get_nearest_target(
    level: &ServerLevel,
    body: &dyn Entity,
    target1: Option<EntityId>,
    target2: EntityId,
) -> EntityId {
    let Some(target1) = target1 else {
        return target2;
    };
    match (level.get_entity(target1), level.get_entity(target2)) {
        (Some(t1), Some(t2)) => {
            get_target_nearest_me(body, target1, t1.position(), target2, t2.position())
        }
        _ => target2,
    }
}

/// `getRandomSwimmablePos(body, maxHorizontalDistance, maxVerticalDistance)`.
pub fn get_random_swimmable_pos(
    body: &mut dyn Entity,
    level: &mut ServerLevel,
    max_horizontal_distance: i32,
    max_vertical_distance: i32,
) -> Option<Vec3> {
    let mut target_pos =
        default_random_pos::get_pos(body, level, max_horizontal_distance, max_vertical_distance);
    let mut count = 0;
    while let Some(pos) = target_pos {
        if level
            .get_block_state(BlockPos::containing(pos.x, pos.y, pos.z))
            .is_pathfindable(PathComputationType::Water)
        {
            break;
        }
        // `count++ < 10`
        count += 1;
        if count > 10 {
            break;
        }
        target_pos = default_random_pos::get_pos(
            body,
            level,
            max_horizontal_distance,
            max_vertical_distance,
        );
    }
    target_pos
}

/// `level.playSound(null, entity, sound, source, volume, pitch)` (seed drawn from the level's sound seed generator); the
/// acting `entity` is out of the level's storage while its behaviors run, so its position comes from the entity itself.
pub fn play_sound_from_entity(
    level: &mut ServerLevel,
    entity: &dyn Entity,
    sound: mc_data::SoundEvent,
    source: mc_protocol::packets::play::SoundSource,
    volume: f32,
    pitch: f32,
) {
    level.play_sound_from_entity_at(
        None,
        entity.id(),
        entity.position(),
        sound,
        source,
        volume,
        pitch,
    );
}
/// `isWithinAttackRange(body, target, projectileAttackRangeMargin)`.
pub fn is_within_attack_range(
    level: &ServerLevel,
    body: &dyn Entity,
    target: EntityId,
    projectile_attack_range_margin: i32,
) -> bool {
    use crate::world::entity::equipment::EquipmentSlot;
    let Some(target) = level.get_entity(target) else {
        return false;
    };
    let main_hand = body.get_item_by_slot(EquipmentSlot::MainHand);
    let default_range = match main_hand.item() {
        mc_data::Item::BOW => Some(crate::world::item::bow_item::DEFAULT_RANGE),
        mc_data::Item::CROSSBOW => Some(crate::world::item::crossbow_item::DEFAULT_RANGE),
        _ => None,
    };
    match default_range {
        Some(range) if body.can_use_non_melee_weapon(&main_hand) => {
            crate::world::entity::ai::sensing::closer_than(
                body,
                target,
                f64::from(range - projectile_attack_range_margin),
            )
        }
        _ => crate::world::entity::mob::combat::is_within_melee_attack_range(body, level, target),
    }
}
