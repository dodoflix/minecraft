//! `VillagerCalmDown`.

use super::village_util::update_activity_from_schedule;
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{
    HURT_BY, HURT_BY_ENTITY, NEAREST_HOSTILE,
};

const SAFE_DISTANCE_FROM_DANGER: f64 = 36.0;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let hurt_by = i.registered(&HURT_BY);
    let hurt_by_entity = i.registered(&HURT_BY_ENTITY);
    let nearest_hostile = i.registered(&NEAREST_HOSTILE);
    i.create(move |c| {
        let body_pos = c.body().position();
        let feel_scared = c.try_get(&hurt_by).is_some()
            || c.try_get(&nearest_hostile).is_some()
            || c.try_get(&hurt_by_entity).is_some_and(|e| {
                c.level.get_entity(e.0).is_some_and(|entity| {
                    entity.base().distance_to_sqr_vec(body_pos) <= SAFE_DISTANCE_FROM_DANGER
                })
            });
        if !feel_scared {
            c.erase(&hurt_by);
            c.erase(&hurt_by_entity);
            update_activity_from_schedule(c.level, c.body);
        }
        true
    })
}
