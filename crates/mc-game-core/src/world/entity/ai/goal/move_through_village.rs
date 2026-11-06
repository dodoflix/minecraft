//! `MoveThroughVillageGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::util::{default_random_pos, land_random_pos};
use crate::world::entity::ai::village::poi::{Occupancy, PoiTypeHolder};
use crate::world::entity::mob::navigation;
use crate::world::level::BlockGetter;
use crate::world::level::block::impls::door_block;
use crate::world::level::pathfinder::Path;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use std::sync::Arc;

/// `BooleanSupplier` over the goal's mob (`canDealWithDoors`).
pub type MobSupplier = Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

pub struct MoveThroughVillageGoal {
    speed_modifier: f64,
    path: Option<Path>,
    poi_pos: BlockPos,
    only_at_night: bool,
    visited: Vec<BlockPos>,
    distance_to_poi: i32,
    can_deal_with_doors: MobSupplier,
}

impl MoveThroughVillageGoal {
    /// `MoveThroughVillageGoal(mob, speedModifier, onlyAtNight, distanceToPoi, canDealWithDoors)`.
    ///
    /// # Panics
    /// `IllegalArgumentException("Unsupported mob for MoveThroughVillageGoal")` for a mob without a ground navigation.
    pub fn new(
        mob: &dyn Entity,
        speed_modifier: f64,
        only_at_night: bool,
        distance_to_poi: i32,
        can_deal_with_doors: MobSupplier,
    ) -> Self {
        assert!(
            mob.mob()
                .and_then(|m| m.navigation.as_deref())
                .is_some_and(|n| n.can_navigate_ground()),
            "Unsupported mob for MoveThroughVillageGoal"
        );
        Self {
            speed_modifier,
            path: None,
            poi_pos: BlockPos::ZERO,
            only_at_night,
            visited: Vec::new(),
            distance_to_poi,
            can_deal_with_doors,
        }
    }

    /// `hasNotVisited(poi)`.
    fn has_not_visited(visited: &[BlockPos], poi: BlockPos) -> bool {
        !visited.contains(&poi)
    }

    /// `updateVisited()`.
    fn update_visited(&mut self) {
        if self.visited.len() > 15 {
            self.visited.remove(0);
        }
    }
}

impl Goal for MoveThroughVillageGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !navigation::can_navigate_ground(mob, level) {
            return false;
        }
        self.update_visited();
        if self.only_at_night && level.is_bright_outside() {
            return false;
        }
        let pos = mob.block_position();
        if !level.is_close_to_village(pos, 6) {
            return false;
        }
        let visited = &self.visited;
        let land_pos =
            land_random_pos::get_pos_weighted(mob, level, 15, 7, &mut |_mob, level, p| {
                if !level.is_village(p) {
                    f64::NEG_INFINITY
                } else {
                    let new_poi_pos = level.poi_manager().find(
                        &|t: PoiTypeHolder| level.poi_type_is_village(t),
                        &|b| Self::has_not_visited(visited, b),
                        p,
                        10,
                        Occupancy::IsOccupied,
                    );
                    new_poi_pos.map_or(f64::NEG_INFINITY, |poi| -poi.dist_sqr(&pos))
                }
            });
        let Some(land_pos) = land_pos else {
            return false;
        };
        let center = BlockPos::containing(land_pos.x, land_pos.y, land_pos.z);
        let target = level.poi_manager().find(
            &|t: PoiTypeHolder| level.poi_type_is_village(t),
            &|b| Self::has_not_visited(&self.visited, b),
            center,
            10,
            Occupancy::IsOccupied,
        );
        let Some(target) = target else { return false };
        self.poi_pos = target;
        let doors = (self.can_deal_with_doors)(mob);
        let poi_pos = self.poi_pos;
        self.path = navigation::with_navigation(mob, level, |n, m, l| {
            n.set_can_open_doors(doors);
            let path = n.create_path_pos(m, &*l, poi_pos, 0);
            n.set_can_open_doors(true);
            path
        })
        .flatten();
        if self.path.is_none() {
            let Some(partial_step) = default_random_pos::get_pos_towards(
                mob,
                level,
                10,
                7,
                Vec3::at_bottom_center_of(&self.poi_pos),
                f64::from((std::f64::consts::PI / 2.0) as f32),
            ) else {
                return false;
            };
            let doors = (self.can_deal_with_doors)(mob);
            self.path = navigation::with_navigation(mob, level, |n, m, l| {
                n.set_can_open_doors(doors);
                let path =
                    n.create_path_xyz(m, &*l, partial_step.x, partial_step.y, partial_step.z, 0);
                n.set_can_open_doors(true);
                path
            })
            .flatten();
            if self.path.is_none() {
                return false;
            }
        }
        if let Some(path) = self.path.clone() {
            for i in 0..path.get_node_count() {
                let node = path.get_node(i);
                let door_pos = BlockPos::new(node.x, node.y + 1, node.z);
                if door_block::is_wooden_door(level.get_block_state(door_pos)) {
                    self.path = navigation::create_path_xyz(
                        mob,
                        level,
                        f64::from(node.x),
                        f64::from(node.y),
                        f64::from(node.z),
                        0,
                    );
                    break;
                }
            }
        }
        self.path.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if navigation::is_done(mob, level) {
            false
        } else {
            !self.poi_pos.closer_to_center_than(
                &mob.position(),
                f64::from(mob.base().get_bb_width() + self.distance_to_poi as f32),
            )
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to_path(mob, level, self.path.clone(), self.speed_modifier);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if navigation::is_done(mob, level)
            || self
                .poi_pos
                .closer_to_center_than(&mob.position(), f64::from(self.distance_to_poi))
        {
            self.visited.push(self.poi_pos);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
