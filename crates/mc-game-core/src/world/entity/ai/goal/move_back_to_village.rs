//! `MoveBackToVillageGoal` (a `RandomStrollGoal`).

use super::{Goal, GoalFlags, RandomStrollGoal};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::util::default_random_pos;
use mc_core::phys::Vec3;
use mc_core::pos::SectionPos;

pub const MAX_XZ_DIST: i32 = 10;
pub const MAX_Y_DIST: i32 = 7;

/// `BehaviorUtils.findSectionClosestToVillage(level, center, radius)`.
pub fn find_section_closest_to_village(
    level: &ServerLevel,
    center: SectionPos,
    radius: i32,
) -> SectionPos {
    let dist_to_village = level.sections_to_village(center);
    SectionPos::cube(&center, radius)
        .filter(|s| level.sections_to_village(*s) < dist_to_village)
        // `Stream.min` keeps the first of equal minima
        .fold(None::<(i32, SectionPos)>, |best, s| {
            let d = level.sections_to_village(s);
            match best {
                Some((bd, _)) if bd <= d => best,
                _ => Some((d, s)),
            }
        })
        .map_or(center, |(_, s)| s)
}

pub struct MoveBackToVillageGoal {
    pub base: RandomStrollGoal,
}

impl MoveBackToVillageGoal {
    /// `MoveBackToVillageGoal(mob, speedModifier, checkNoActionTime)`.
    pub fn new(speed_modifier: f64, check_no_action_time: bool) -> Self {
        Self {
            base: RandomStrollGoal::with_check(speed_modifier, 10, check_no_action_time),
        }
    }

    /// `getPosition()`.
    pub fn get_position(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        let pos = mob.block_position();
        let section_pos = SectionPos::of_block_pos(&pos);
        let optimal = find_section_closest_to_village(level, section_pos, 2);
        if optimal != section_pos {
            default_random_pos::get_pos_towards(
                mob,
                level,
                MAX_XZ_DIST,
                MAX_Y_DIST,
                Vec3::at_bottom_center_of(&optimal.center()),
                f64::from((std::f64::consts::PI / 2.0) as f32),
            )
        } else {
            None
        }
    }
}

impl Goal for MoveBackToVillageGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if level.is_village(mob.block_position()) {
            false
        } else {
            self.base.can_use_with(mob, level, Self::get_position)
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.stop_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
