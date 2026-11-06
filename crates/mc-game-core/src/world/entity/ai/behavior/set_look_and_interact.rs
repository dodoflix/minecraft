//! `SetLookAndInteract`.

use super::position_tracker::{EntityTracker, PositionTracker};
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::EntityRef;
use crate::world::entity::ai::memory::memory_module_type::{
    INTERACTION_TARGET, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use mc_data::EntityType;

/// `create(type, interactionRange)`.
pub fn create(ty: EntityType, interaction_range: i32) -> OneShot {
    let interaction_range_sqr = interaction_range * interaction_range;
    let mut i = BehaviorBuilder::instance();
    let look_target = i.registered(&LOOK_TARGET);
    let interaction_target = i.absent(&INTERACTION_TARGET);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let entities = c.get(&nearest_entities).clone();
        let body_pos = c.body().position();
        let closest = entities.find_closest(c.level, c.body(), |e| {
            e.base().distance_to_sqr_vec(body_pos) <= f64::from(interaction_range_sqr)
                && e.get_type() == ty
        });
        let Some(EntityRef(id)) = closest else {
            return false;
        };
        let Some(entity) = c.level.get_entity(id) else {
            return false;
        };
        let tracker = PositionTracker::Entity(EntityTracker::new(entity, true));
        c.set(&interaction_target, EntityRef(id));
        c.set(&look_target, tracker);
        true
    })
}
