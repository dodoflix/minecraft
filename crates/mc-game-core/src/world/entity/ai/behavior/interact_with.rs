//! `InteractWith`: walk up to the closest entity of a type and remember it as the interaction target.

use super::position_tracker::{EntityTracker, PositionTracker};
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::Entity;
use crate::world::entity::ai::goal::target::EntityClass;
use crate::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET,
};
use crate::world::entity::ai::memory::{EntityRef, MemoryModuleType, WalkTarget};
use mc_data::EntityType;
use std::sync::Arc;

/// `of(type, interactionRange, interactionTarget, speedModifier, stopDistance)`.
pub fn of(
    ty: EntityType,
    interaction_range: i32,
    interaction_target: MemoryModuleType<EntityRef>,
    speed_modifier: f32,
    stop_distance: i32,
) -> OneShot {
    of_filtered(
        ty,
        interaction_range,
        Arc::new(|_| true),
        Arc::new(|_| true),
        interaction_target,
        speed_modifier,
        stop_distance,
    )
}

/// `of(type, interactionRange, selfFilter, targetFilter, interactionTarget, speedModifier, stopDistance)`.
pub fn of_filtered(
    ty: EntityType,
    interaction_range: i32,
    self_filter: EntityClass,
    target_filter: EntityClass,
    interaction_target: MemoryModuleType<EntityRef>,
    speed_modifier: f32,
    stop_distance: i32,
) -> OneShot {
    let interaction_range_sqr = interaction_range * interaction_range;
    let is_target_valid = move |mob: &dyn Entity| mob.get_type() == ty && target_filter(mob);
    let mut i = BehaviorBuilder::instance();
    let target = i.registered(&interaction_target);
    let look_target = i.registered(&LOOK_TARGET);
    let walk_target = i.absent(&WALK_TARGET);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let entities = c.get(&nearest_entities).clone();
        if self_filter(c.body()) && entities.contains_matching(c.level, c.body(), &is_target_valid)
        {
            let body_pos = c.body().position();
            let closest = entities.find_closest(c.level, c.body(), |mob| {
                mob.base().distance_to_sqr_vec(body_pos) <= f64::from(interaction_range_sqr)
                    && is_target_valid(mob)
            });
            if let Some(EntityRef(id)) = closest
                && let Some(mob) = c.level.get_entity_or_recently_removed(id)
            {
                let look = PositionTracker::Entity(EntityTracker::new(mob, true));
                let walk = WalkTarget::new(
                    PositionTracker::Entity(EntityTracker::new(mob, false)),
                    speed_modifier,
                    stop_distance,
                );
                c.set(&target, EntityRef(id));
                c.set(&look_target, look);
                c.set(&walk_target, walk);
            }
            true
        } else {
            false
        }
    })
}
