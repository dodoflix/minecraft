//! `RestrictSunGoal`.

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::ai::util::goal_utils;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob::navigation;

#[derive(Default)]
pub struct RestrictSunGoal;

impl RestrictSunGoal {
    pub fn new() -> Self {
        Self
    }

    /// `if (getNavigation() instanceof GroundPathNavigation nav) nav.setAvoidSun(avoid)`.
    fn set_avoid_sun(mob: &mut dyn Entity, level: &mut ServerLevel, avoid: bool) {
        navigation::with_navigation(mob, level, |n, _, _| {
            if n.data().class.is_ground() {
                n.data_mut().set_avoid_sun(avoid);
            }
        });
    }
}

impl Goal for RestrictSunGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        level.is_bright_outside()
            && mob.get_item_by_slot(EquipmentSlot::Head).is_empty()
            && goal_utils::has_ground_path_navigation(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        Self::set_avoid_sun(mob, level, true);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if goal_utils::has_ground_path_navigation(mob, level) {
            Self::set_avoid_sun(mob, level, false);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
