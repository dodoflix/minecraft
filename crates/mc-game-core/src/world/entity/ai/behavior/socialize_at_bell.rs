//! `SocializeAtBell`.

use mc_core::random::RandomSource;
use super::position_tracker::{EntityTracker, PositionTracker};
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{INTERACTION_TARGET, LOOK_TARGET, MEETING_POINT, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET};
use crate::world::entity::ai::memory::EntityRef;
use crate::world::level::LevelAccessor as _;
use mc_data::EntityType;

const SPEED_MODIFIER: f32 = 0.3;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.registered(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    let meeting_point = i.present(&MEETING_POINT);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    let interaction_target = i.absent(&INTERACTION_TARGET);
    i.create(move |c| {
        let memory = c.get(&meeting_point).clone();
        let visible_entities = c.get(&nearest_entities).clone();
        if c.level.random().next_int_bounded(100) == 0
            && c.level.dimension == memory.dimension
            && memory.pos.closer_to_center_than(&c.body().position(), 4.0)
            && visible_entities.contains_matching(c.level, c.body(), |mob| mob.get_type() == EntityType::VILLAGER)
        {
            let body_pos = c.body().position();
            let closest = {
                let body = c.body();
                visible_entities.find_closest(c.level, body, |mob| mob.get_type() == EntityType::VILLAGER && mob.base().distance_to_sqr_vec(body_pos) <= 32.0)
            };
            if let Some(EntityRef(mob_id)) = closest {
                if let Some(mob) = c.level.get_entity(mob_id) {
                    let look = PositionTracker::Entity(EntityTracker::new(mob, true));
                    let walk = WalkTarget::new(PositionTracker::Entity(EntityTracker::new(mob, false)), SPEED_MODIFIER, 1);
                    c.set(&interaction_target, EntityRef(mob_id));
                    c.set(&look_target, look);
                    c.set(&walk_target, walk);
                }
            }
            true
        } else {
            false
        }
    })
}
