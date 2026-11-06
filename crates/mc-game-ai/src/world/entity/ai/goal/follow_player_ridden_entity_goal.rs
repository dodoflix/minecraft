//! `FollowPlayerRiddenEntityGoal`.

use mc_core::phys::Vec3;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::living_entity::util::l;
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::entity::{Entity, EntityId, MoverType};

/// `FollowPlayerRiddenEntityGoal.FollowEntityGoal`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FollowEntityGoal {
    GoToEntity,
    GoInEntityDirection,
}

/// `Entity.hasMovedHorizontallyRecently()`.
fn has_moved_horizontally_recently(e: &dyn Entity, level: &ServerLevel) -> bool {
    e.get_known_speed(level).horizontal_distance().abs() > f64::from(1.0E-5f32)
}

/// `FollowPlayerRiddenEntityGoal(mob, entityTypeToFollow)`; `type_to_follow` is the `Class<? extends Entity>` test.
pub struct FollowPlayerRiddenEntityGoal {
    time_to_recalc_path: i32,
    type_to_follow: fn(&dyn Entity) -> bool,
    following: Option<EntityId>,
    current_goal: FollowEntityGoal,
}

impl FollowPlayerRiddenEntityGoal {
    pub fn new(type_to_follow: fn(&dyn Entity) -> bool) -> Self {
        Self {
            time_to_recalc_path: 0,
            type_to_follow,
            following: None,
            current_goal: FollowEntityGoal::GoToEntity,
        }
    }

    fn nearby(&self, mob: &dyn Entity, level: &ServerLevel) -> Vec<EntityId> {
        let bb = mob.base().bounding_box().inflate(5.0, 5.0, 5.0);
        level.get_entities_of_class(&bb, self.type_to_follow, |_| true)
    }
}

/// `distanceTo(entity)`: float math like `Entity.distanceTo`.
fn distance_to(a: &dyn Entity, b: &dyn Entity) -> f32 {
    let dx = (a.base().x() - b.base().x()) as f32;
    let dy = (a.base().y() - b.base().y()) as f32;
    let dz = (a.base().z() - b.base().z()) as f32;
    mc_core::math::mth::sqrt(dx * dx + dy * dy + dz * dz)
}

impl Goal for FollowPlayerRiddenEntityGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self
            .following
            .and_then(|id| level.get_entity(id))
            .is_some_and(|f| has_moved_horizontally_recently(f, level))
        {
            return true;
        }
        for id in self.nearby(mob, level) {
            let Some(entity) = level.get_entity(id) else {
                continue;
            };
            if let Some(controller) = entity
                .get_controlling_passenger()
                .and_then(|c| level.get_entity(c))
                && controller.is_player()
                && has_moved_horizontally_recently(controller, level)
            {
                return true;
            }
        }
        false
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.following
            .and_then(|id| level.get_entity(id))
            .is_some_and(|f| f.base().is_passenger() && has_moved_horizontally_recently(f, level))
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        for id in self.nearby(mob, level) {
            let Some(entity) = level.get_entity(id) else {
                continue;
            };
            if let Some(controller) = entity
                .get_controlling_passenger()
                .filter(|c| level.get_entity(*c).is_some_and(|p| p.is_player()))
            {
                self.following = Some(controller);
                break;
            }
        }
        self.time_to_recalc_path = 0;
        self.current_goal = FollowEntityGoal::GoToEntity;
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.following = None;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let speed = if self.current_goal == FollowEntityGoal::GoInEntityDirection {
            0.01f32
        } else {
            0.015f32
        };
        let (xxa, yya, zza) = {
            let living = l(mob);
            (living.xxa, living.yya, living.zza)
        };
        mob.move_relative(
            speed,
            Vec3::new(f64::from(xxa), f64::from(yya), f64::from(zza)),
        );
        let dm = mob.base().delta_movement();
        mob.r#move(level, MoverType::SelfMover, dm);
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path <= 0 {
            self.time_to_recalc_path = self.adjusted_tick_delay(10);
            let Some(following) = self.following else {
                return;
            };
            let Some((pos, direction, motion_direction)) = level
                .get_entity(following)
                .map(|f| (f.block_position(), f.get_direction(), f.get_direction()))
            else {
                return;
            };
            let far = level.get_entity(following).map(|f| distance_to(mob, f));
            match self.current_goal {
                FollowEntityGoal::GoToEntity => {
                    let behind = pos.relative(direction.get_opposite()).offset(0, -1, 0);
                    navigation::move_to(
                        mob,
                        level,
                        f64::from(behind.x()),
                        f64::from(behind.y()),
                        f64::from(behind.z()),
                        1.0,
                    );
                    if far.is_some_and(|d| d < 4.0) {
                        self.time_to_recalc_path = 0;
                        self.current_goal = FollowEntityGoal::GoInEntityDirection;
                    }
                }
                FollowEntityGoal::GoInEntityDirection => {
                    let go_to = pos.relative_n(motion_direction, 10);
                    navigation::move_to(
                        mob,
                        level,
                        f64::from(go_to.x()),
                        f64::from(go_to.y() - 1),
                        f64::from(go_to.z()),
                        1.0,
                    );
                    if far.is_some_and(|d| d > 12.0) {
                        self.time_to_recalc_path = 0;
                        self.current_goal = FollowEntityGoal::GoToEntity;
                    }
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
