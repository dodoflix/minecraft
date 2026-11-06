//! `BreathAirGoal`.

use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, PathComputationType};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::entity::{Entity, MoverType};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;

#[derive(Default)]
pub struct BreathAirGoal;

impl BreathAirGoal {
    pub fn new() -> Self {
        Self
    }

    fn find_air_position(mob: &mut dyn Entity, level: &mut ServerLevel) {
        let p = mob.block_position();
        let destination = BlockPos::neighbor_column(p.x(), p.y(), p.z(), p.y() + 8)
            .find(|pos| Self::gives_air(level, *pos));
        let destination = destination.unwrap_or_else(|| {
            BlockPos::containing(mob.base().x(), mob.base().y() + 8.0, mob.base().z())
        });
        navigation::move_to(
            mob,
            level,
            f64::from(destination.x()),
            f64::from(destination.y() + 1),
            f64::from(destination.z()),
            1.0,
        );
    }

    fn gives_air(level: &ServerLevel, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        (level.get_fluid_state(pos).is_empty() || state.block() == Block::BUBBLE_COLUMN)
            && state.is_pathfindable_by(PathComputationType::Land)
    }
}

impl Goal for BreathAirGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.base().get_air_supply() < 140
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use(mob, level)
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
        Self::find_air_position(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        Self::find_air_position(mob, level);
        let (xxa, yya, zza) = mob
            .living()
            .map_or((0.0, 0.0, 0.0), |l| (l.xxa, l.yya, l.zza));
        mob.move_relative(
            0.02,
            Vec3::new(f64::from(xxa), f64::from(yya), f64::from(zza)),
        );
        let delta = mob.base().delta_movement();
        mob.r#move(level, MoverType::SelfMover, delta);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
