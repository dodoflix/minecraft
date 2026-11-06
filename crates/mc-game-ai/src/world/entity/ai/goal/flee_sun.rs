//! `FleeSunGoal`.

use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob::{navigation, tick};
use mc_game_core::world::level::LevelReader;

pub struct FleeSunGoal {
    wanted_x: f64,
    wanted_y: f64,
    wanted_z: f64,
    speed_modifier: f64,
}

impl FleeSunGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            wanted_x: 0.0,
            wanted_y: 0.0,
            wanted_z: 0.0,
            speed_modifier,
        }
    }

    /// `setWantedPos()`.
    pub fn set_wanted_pos(&mut self, mob: &mut dyn Entity, level: &ServerLevel) -> bool {
        match Self::get_hide_pos(mob, level) {
            None => false,
            Some(pos) => {
                self.wanted_x = pos.x;
                self.wanted_y = pos.y;
                self.wanted_z = pos.z;
                true
            }
        }
    }

    /// `getHidePos()`.
    pub fn get_hide_pos(mob: &mut dyn Entity, level: &ServerLevel) -> Option<Vec3> {
        let pos = mob.block_position();
        for _ in 0..10 {
            let random = &mut mob.base_mut().random;
            let (dx, dy, dz) = (
                random.next_int_bounded(20) - 10,
                random.next_int_bounded(6) - 3,
                random.next_int_bounded(20) - 10,
            );
            let random_pos = pos.offset(dx, dy, dz);
            if !level.can_see_sky(random_pos) && mob.get_walk_target_value(random_pos, level) < 0.0
            {
                return Some(Vec3::at_bottom_center_of(&random_pos));
            }
        }
        None
    }
}

impl Goal for FleeSunGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if tick::get_target_checked(mob, level).is_some()
            || !level.is_bright_outside()
            || !mob.is_on_fire()
            || !level.can_see_sky(mob.block_position())
            || !mob.get_item_by_slot(EquipmentSlot::Head).is_empty()
        {
            false
        } else {
            self.set_wanted_pos(mob, level)
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !navigation::is_done(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(
            mob,
            level,
            self.wanted_x,
            self.wanted_y,
            self.wanted_z,
            self.speed_modifier,
        );
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
