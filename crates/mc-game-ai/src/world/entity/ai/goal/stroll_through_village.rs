//! `StrollThroughVillageGoal`.

use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, SectionPos};
use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::mob::navigation;
use mc_world::heightmap::HeightmapType;

pub struct StrollThroughVillageGoal {
    interval: i32,
    wanted_pos: Option<BlockPos>,
}

impl StrollThroughVillageGoal {
    /// `StrollThroughVillageGoal(mob, interval)`.
    pub fn new(interval: i32) -> Self {
        Self {
            interval: reduced_tick_delay(interval),
            wanted_pos: None,
        }
    }

    fn move_randomly(mob: &mut dyn Entity, level: &mut ServerLevel) {
        let random = &mut mob.base_mut().random;
        let (dx, dz) = (
            -8 + random.next_int_bounded(16),
            -8 + random.next_int_bounded(16),
        );
        let path_target = level.get_heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            mob.block_position().offset(dx, 0, dz),
        );
        navigation::move_to(
            mob,
            level,
            f64::from(path_target.x()),
            f64::from(path_target.y()),
            f64::from(path_target.z()),
            1.0,
        );
    }
}

impl Goal for StrollThroughVillageGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.has_controlling_passenger()
            || level.is_bright_outside()
            || mob.base_mut().random.next_int_bounded(self.interval) != 0
        {
            false
        } else {
            let pos = mob.block_position();
            if !level.is_close_to_village(pos, 6) {
                return false;
            }
            let land_pos =
                land_random_pos::get_pos_weighted(mob, level, 15, 7, &mut |_m, level, p| {
                    -f64::from(level.sections_to_village(SectionPos::of_block_pos(&p)))
                });
            self.wanted_pos = land_pos.map(|p| BlockPos::containing(p.x, p.y, p.z));
            self.wanted_pos.is_some()
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let target = navigation::with_navigation_ref(mob, level, |n| n.get_target_pos()).flatten();
        self.wanted_pos.is_some() && !navigation::is_done(mob, level) && target == self.wanted_pos
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(wanted_pos) = self.wanted_pos else {
            return;
        };
        if navigation::is_done(mob, level)
            && !wanted_pos.closer_to_center_than(&mob.position(), 10.0)
        {
            let mut long_distance_target = Vec3::at_bottom_center_of(&wanted_pos);
            let self_vector = mob.position();
            let distance = self_vector.subtract(long_distance_target);
            long_distance_target = distance.scale(0.4).add(long_distance_target);
            let move_target = long_distance_target
                .subtract(self_vector)
                .normalize()
                .scale(10.0)
                .add(self_vector);
            let path_target = BlockPos::containing(move_target.x, move_target.y, move_target.z);
            let path_target =
                level.get_heightmap_pos(HeightmapType::MotionBlockingNoLeaves, path_target);
            if !navigation::move_to(
                mob,
                level,
                f64::from(path_target.x()),
                f64::from(path_target.y()),
                f64::from(path_target.z()),
                1.0,
            ) {
                Self::move_randomly(mob, level);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
