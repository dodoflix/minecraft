//! `FollowOwnerGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::AnyDimensionEntity;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::EntityId;
use crate::world::entity::mob::{self, navigation, tamable};
use crate::world::level::pathfinder::PathType;
use uuid::Uuid;

pub struct FollowOwnerGoal {
    /// The owner (uuid, and id for an owner of this level that died), found in any level (`getOwner()`).
    owner: Option<(Uuid, EntityId)>,
    speed_modifier: f64,
    time_to_recalc_path: i32,
    stop_distance: f32,
    start_distance: f32,
    old_water_cost: f32,
}

impl FollowOwnerGoal {
    /// `FollowOwnerGoal(tamable, speedModifier, startDistance, stopDistance)` (the navigation must be a ground or flying one).
    pub fn new(speed_modifier: f64, start_distance: f32, stop_distance: f32) -> Self {
        Self {
            owner: None,
            speed_modifier,
            time_to_recalc_path: 0,
            stop_distance,
            start_distance,
            old_water_cost: 0.0,
        }
    }
}

impl FollowOwnerGoal {
    /// The owner this goal follows, as long as its level still has it (a dead owner of this level stays for a tick).
    fn owner_entity<'l>(&self, level: &'l ServerLevel) -> Option<AnyDimensionEntity<'l>> {
        let (uuid, id) = self.owner?;
        level.get_entity_in_any_dimension(uuid).or_else(|| {
            level
                .get_entity_or_recently_removed(id)
                .map(AnyDimensionEntity::Here)
        })
    }
}

impl Goal for FollowOwnerGoal {
    fn can_use(&mut self, tamable: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(owner) = tamable::get_owner_in_any_dimension(tamable, level) else {
            return false;
        };
        if tamable::unable_to_move_to_owner(tamable, level) {
            return false;
        }
        if tamable.base().distance_to_sqr_vec(owner.position())
            < f64::from(self.start_distance * self.start_distance)
        {
            return false;
        }
        self.owner = Some((owner.uuid(), owner.id()));
        true
    }

    fn can_continue_to_use(&mut self, tamable: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if navigation::is_done(tamable, level) {
            return false;
        }
        if tamable::unable_to_move_to_owner(tamable, level) {
            return false;
        }
        let Some(owner) = self.owner_entity(level) else {
            return false;
        };
        !(tamable.base().distance_to_sqr_vec(owner.position())
            <= f64::from(self.stop_distance * self.stop_distance))
    }

    fn start(&mut self, tamable: &mut dyn Entity, level: &mut ServerLevel) {
        self.time_to_recalc_path = 0;
        self.old_water_cost = mob::get_pathfinding_malus(tamable, level, PathType::Water);
        mob::set_pathfinding_malus(tamable, PathType::Water, 0.0);
    }

    fn stop(&mut self, tamable: &mut dyn Entity, level: &mut ServerLevel) {
        self.owner = None;
        navigation::stop(tamable, level);
        mob::set_pathfinding_malus(tamable, PathType::Water, self.old_water_cost);
    }

    fn tick(&mut self, tamable: &mut dyn Entity, level: &mut ServerLevel) {
        let is_owner_far_away = tamable::should_try_teleport_to_owner(tamable, level);
        if !is_owner_far_away {
            let max_x = tamable.get_max_head_x_rot() as f32;
            match self.owner_entity(level) {
                Some(AnyDimensionEntity::Here(owner)) => {
                    mob::look_at_entity_full(tamable, owner, 10.0, max_x);
                }
                Some(AnyDimensionEntity::Elsewhere(owner)) => {
                    mob::look_at_position_full(
                        tamable,
                        owner.position.x,
                        owner.eye_y,
                        owner.position.z,
                        10.0,
                        max_x,
                    );
                }
                None => {}
            }
        }
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path <= 0 {
            self.time_to_recalc_path = self.adjusted_tick_delay(10);
            if is_owner_far_away {
                tamable::try_to_teleport_to_owner(tamable, level);
            } else if let Some(pos) = self.owner_entity(level).map(|o| o.block_position()) {
                navigation::move_to_entity(tamable, level, pos, self.speed_modifier);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
