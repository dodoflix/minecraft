//! `FollowParentGoal`.

use super::{Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob::{ageable, navigation};
use crate::world::entity::{Entity, EntityId};
use std::any::TypeId;
use std::sync::Arc;

pub const HORIZONTAL_SCAN_RANGE: i32 = 8;
pub const VERTICAL_SCAN_RANGE: i32 = 4;
pub const DONT_FOLLOW_IF_CLOSER_THAN: i32 = 3;

pub struct FollowParentGoal {
    parent: Option<EntityId>,
    speed_modifier: f64,
    time_to_recalc_path: i32,
    class: ParentClass,
}

/// The class of the parents: the animal's own concrete type, or an arbitrary `instanceof` test.
enum ParentClass {
    Exact(TypeId),
    Test(Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>),
}

impl FollowParentGoal {
    /// `FollowParentGoal(animal, speedModifier)`: parents are animals of `animal`'s own class.
    pub fn new(animal: &dyn Entity, speed_modifier: f64) -> Self {
        Self { parent: None, speed_modifier, time_to_recalc_path: 0, class: ParentClass::Exact(animal.as_any().type_id()) }
    }

    /// `FollowParentGoal(animal, speedModifier)` for a class that spans several Rust types or entity types (`Llama`/`TraderLlama`).
    pub fn with_class_test(speed_modifier: f64, class: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>) -> Self {
        Self { parent: None, speed_modifier, time_to_recalc_path: 0, class: ParentClass::Test(class) }
    }
}

impl Goal for FollowParentGoal {
    fn can_use(&mut self, animal: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if ageable::get_age(animal) >= 0 {
            return false;
        }
        let bb = animal.base().bounding_box().inflate(8.0, 4.0, 8.0);
        let class = &self.class;
        let parents = level.get_entities_of_class(
            &bb,
            |e| match class {
                ParentClass::Exact(type_id) => e.as_any().type_id() == *type_id,
                ParentClass::Test(test) => test(e),
            },
            |_| true,
        );
        let mut closest: Option<EntityId> = None;
        let mut closest_dist_sqr = f64::MAX;
        for id in parents {
            let Some(parent) = level.get_entity(id) else { continue };
            if ageable::get_age(parent) >= 0 {
                let dist_sqr = animal.base().distance_to_sqr_vec(parent.position());
                if !(dist_sqr > closest_dist_sqr) {
                    closest_dist_sqr = dist_sqr;
                    closest = Some(id);
                }
            }
        }
        match closest {
            None => false,
            Some(_) if closest_dist_sqr < 9.0 => false,
            Some(id) => {
                self.parent = Some(id);
                true
            }
        }
    }

    fn can_continue_to_use(&mut self, animal: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if ageable::get_age(animal) >= 0 {
            return false;
        }
        let Some(parent) = self.parent.and_then(|p| level.get_entity(p)) else { return false };
        if !parent.is_alive() {
            return false;
        }
        let dist_sqr = animal.base().distance_to_sqr_vec(parent.position());
        !(dist_sqr < 9.0) && !(dist_sqr > 256.0)
    }

    fn start(&mut self, _animal: &mut dyn Entity, _level: &mut ServerLevel) {
        self.time_to_recalc_path = 0;
    }

    fn stop(&mut self, _animal: &mut dyn Entity, _level: &mut ServerLevel) {
        self.parent = None;
    }

    fn tick(&mut self, animal: &mut dyn Entity, level: &mut ServerLevel) {
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path <= 0 {
            self.time_to_recalc_path = self.adjusted_tick_delay(10);
            if let Some(pos) = self.parent.and_then(|p| level.get_entity(p)).map(|p| p.block_position()) {
                navigation::move_to_entity(animal, level, pos, self.speed_modifier);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
