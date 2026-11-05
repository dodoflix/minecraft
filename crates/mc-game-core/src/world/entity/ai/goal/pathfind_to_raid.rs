//! `PathfindToRaidGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::util::java_hash_order::JavaHashCapacity;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::mob::{navigation, tick};
use crate::world::entity::raid::raider::{get_current_raid, has_active_raid};
use crate::world::entity::raid::raids::can_join_raid;
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::Vec3;

const RECRUITMENT_SEARCH_TICK_DELAY: i32 = 20;

pub struct PathfindToRaidGoal {
    recruitment_tick: i32,
}

impl PathfindToRaidGoal {
    pub fn new() -> Self {
        PathfindToRaidGoal { recruitment_tick: 0 }
    }

    /// `recruitNearby(raid)`.
    fn recruit_nearby(mob: &dyn Entity, level: &mut ServerLevel, raid_id: i32) {
        if !level.get_raid(raid_id).is_some_and(|r| r.is_active()) {
            return;
        }
        let bb = mob.get_bounding_box().inflate_scalar(16.0);
        let nearby: Vec<EntityId> = level.get_entities_of_class(&bb, |e| e.raider().is_some(), |e| !has_active_raid(e, level) && can_join_raid(e));
        // `Sets.newHashSet()` of raiders: iterated in hash (entity id) order
        let mut capacity = JavaHashCapacity::default();
        capacity.grow_to_fit(nearby.len());
        for raider_id in capacity.iteration_order(nearby, |id| *id) {
            level.with_raid(raid_id, |level, raid| {
                let groups = raid.get_groups_spawned();
                level.with_entity(raider_id, |level, raider| raid.join_raid_existing(level, groups, raider));
            });
        }
    }
}

impl Default for PathfindToRaidGoal {
    fn default() -> Self {
        Self::new()
    }
}

fn raid_ongoing_outside_village(mob: &dyn Entity, level: &ServerLevel) -> bool {
    has_active_raid(mob, level) && !get_current_raid(mob).and_then(|id| level.get_raid(id)).is_some_and(|r| r.is_over()) && !crate::world::entity::raid::raid_world::is_village(level, mob.block_position())
}

impl Goal for PathfindToRaidGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        tick::get_target_checked(mob, level).is_none() && !mob.has_controlling_passenger() && raid_ongoing_outside_village(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        raid_ongoing_outside_village(mob, level)
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if has_active_raid(mob, level) {
            let raid_id = get_current_raid(mob).expect("active raid");
            if mob.base().tick_count > self.recruitment_tick {
                self.recruitment_tick = mob.base().tick_count.wrapping_add(RECRUITMENT_SEARCH_TICK_DELAY);
                Self::recruit_nearby(mob, level, raid_id);
            }
            if !navigation::is_in_progress(mob, level) {
                let center = level.get_raid(raid_id).expect("active raid").get_center();
                let pos_towards = default_random_pos::get_pos_towards(mob, level, 15, 4, Vec3::at_bottom_center_of(&center), f64::from(std::f32::consts::PI / 2.0));
                if let Some(p) = pos_towards {
                    navigation::move_to(mob, level, p.x, p.y, p.z, 1.0);
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
