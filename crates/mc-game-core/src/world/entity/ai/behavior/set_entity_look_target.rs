//! `SetEntityLookTarget`.

use super::OneShot;
use super::declarative::BehaviorBuilder;
use super::position_tracker::{EntityTracker, PositionTracker};
use crate::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use crate::world::entity::ai::sensing::distance_to_sqr;
use crate::world::entity::entity::Entity;
use mc_data::EntityType;
use mc_data::enums::MobCategory;

/// `create(category, maxDist)`.
pub fn create_category(category: MobCategory, max_dist: f32) -> OneShot {
    create_with(
        move |mob| mob.get_type().data().category == category,
        max_dist,
    )
}

/// `create(type, maxDist)`.
pub fn create_type(ty: EntityType, max_dist: f32) -> OneShot {
    create_with(move |mob| mob.get_type() == ty, max_dist)
}

/// `create(maxDist)`.
pub fn create(max_dist: f32) -> OneShot {
    create_with(|_| true, max_dist)
}

/// `create(predicate, maxDist)`.
pub fn create_with(predicate: impl Fn(&dyn Entity) -> bool + 'static, max_dist: f32) -> OneShot {
    let max_dist_sqr = max_dist * max_dist;
    let mut i = BehaviorBuilder::instance();
    let look_target = i.absent(&LOOK_TARGET);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let body: &dyn Entity = &*c.body;
        let target = c
            .get(&nearest_entities)
            .find_closest(&*c.level, body, |mob| {
                predicate(mob)
                    && distance_to_sqr(mob, body) <= f64::from(max_dist_sqr)
                    && !body.base().passengers.contains(&mob.id())
            });
        let Some(target) = target else { return false };
        let Some(tracker) = c
            .level
            .get_entity_or_recently_removed(target.0)
            .map(|e| EntityTracker::new(e, true))
        else {
            return false;
        };
        c.set(&look_target, PositionTracker::Entity(tracker));
        true
    })
}
