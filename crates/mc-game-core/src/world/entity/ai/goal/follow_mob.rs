//! `FollowMobGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::navigation::NavClass;
use crate::world::entity::mob::{self, leashable, navigation};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::pathfinder::PathType;
use std::any::TypeId;

pub struct FollowMobGoal {
    /// `followPredicate`: `mob.getClass() != input.getClass()`.
    own_class: TypeId,
    following_mob: Option<EntityId>,
    speed_modifier: f64,
    time_to_recalc_path: i32,
    stop_distance: f32,
    old_water_cost: f32,
    area_size: f32,
}

impl FollowMobGoal {
    /// `FollowMobGoal(mob, speedModifier, stopDistance, areaSize)`.
    ///
    /// # Panics
    /// `IllegalArgumentException("Unsupported mob type for FollowMobGoal")` for a navigation that is neither ground nor flying.
    pub fn new(mob: &dyn Entity, speed_modifier: f64, stop_distance: f32, area_size: f32) -> Self {
        let class = mob
            .mob()
            .and_then(|m| m.navigation.as_deref())
            .map(|n| n.data().class);
        assert!(
            class.is_some_and(|c| c.is_ground() || c == NavClass::Flying),
            "Unsupported mob type for FollowMobGoal"
        );
        Self {
            own_class: mob.as_any().type_id(),
            following_mob: None,
            speed_modifier,
            time_to_recalc_path: 0,
            stop_distance,
            old_water_cost: 0.0,
            area_size,
        }
    }
}

impl Goal for FollowMobGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let size = f64::from(self.area_size);
        let bb = mob.base().bounding_box().inflate(size, size, size);
        let own = self.own_class;
        let mobs = level.get_entities_of_class(
            &bb,
            |e| e.mob().is_some(),
            |e| e.as_any().type_id() != own,
        );
        for id in mobs {
            if level.get_entity(id).is_some_and(|m| !m.is_invisible()) {
                self.following_mob = Some(id);
                return true;
            }
        }
        false
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(f) = self
            .following_mob
            .and_then(|f| level.get_entity_or_recently_removed(f))
        else {
            return false;
        };
        !navigation::own_is_done(mob)
            && mob.base().distance_to_sqr_vec(f.position())
                > f64::from(self.stop_distance * self.stop_distance)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.time_to_recalc_path = 0;
        self.old_water_cost = mob::get_pathfinding_malus(mob, level, PathType::Water);
        mob::set_pathfinding_malus(mob, PathType::Water, 0.0);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.following_mob = None;
        if let Some(n) = mob.mob_mut().and_then(|m| m.navigation.as_deref_mut()) {
            n.stop();
        }
        mob::set_pathfinding_malus(mob, PathType::Water, self.old_water_cost);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(following) = self.following_mob else {
            return;
        };
        if leashable::is_leashed(mob) {
            return;
        }
        let max_head_x = mob.get_max_head_x_rot() as f32;
        {
            let Some(f) = level.get_entity_or_recently_removed(following) else {
                return;
            };
            mob::look_at_entity_full(mob, f, 10.0, max_head_x);
        }
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path <= 0 {
            self.time_to_recalc_path = self.adjusted_tick_delay(10);
            let Some(f) = level.get_entity_or_recently_removed(following) else {
                return;
            };
            let (fx, fy, fz) = (f.base().x(), f.base().y(), f.base().z());
            let (mx, my, mz) = (mob.base().x(), mob.base().y(), mob.base().z());
            let (xxd, yyd, zzd) = (mx - fx, my - fy, mz - fz);
            let dist_sqr = xxd * xxd + yyd * yyd + zzd * zzd;
            let stop_sqr = f64::from(self.stop_distance * self.stop_distance);
            let speed = self.speed_modifier;
            if !(dist_sqr <= stop_sqr) {
                let pos = f.block_position();
                navigation::with_own_navigation(mob, level, |n, m, l| {
                    n.move_to_entity(m, &*l, pos, speed)
                });
            } else {
                let looks_at_us =
                    f.mob()
                        .and_then(|m| m.look_control.as_deref())
                        .is_some_and(|lc| {
                            lc.get_wanted_x() == mx
                                && lc.get_wanted_y() == my
                                && lc.get_wanted_z() == mz
                        });
                navigation::with_own_navigation(mob, level, |n, _, _| n.stop());
                if dist_sqr <= f64::from(self.stop_distance) || looks_at_us {
                    let (delta_x, delta_z) = (fx - mx, fz - mz);
                    navigation::with_own_navigation(mob, level, |n, m, l| {
                        n.move_to_xyz(m, &*l, mx - delta_x, my, mz - delta_z, speed)
                    });
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
