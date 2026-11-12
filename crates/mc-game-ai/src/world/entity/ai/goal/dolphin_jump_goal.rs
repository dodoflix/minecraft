//! `DolphinJumpGoal` (+ the abstract `JumpGoal` flags).

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::level_access::BlockGetter as _;
use mc_game_core::world::level::material::FluidStateExt as _;

const STEPS_TO_CHECK: [i32; 6] = [0, 1, 4, 5, 6, 7];

/// `DolphinJumpGoal(dolphin, interval)`.
pub struct DolphinJumpGoal {
    interval: i32,
    breached: bool,
}

impl DolphinJumpGoal {
    pub fn new(interval: i32) -> Self {
        Self {
            interval: reduced_tick_delay(interval),
            breached: false,
        }
    }

    fn water_is_clear(
        level: &ServerLevel,
        dolphin_pos: BlockPos,
        step_x: i32,
        step_z: i32,
        current_step: i32,
    ) -> bool {
        let next_pos = dolphin_pos.offset(step_x * current_step, 0, step_z * current_step);
        level.get_fluid_state(next_pos).is_tag("water")
            && !level
                .get_block_state(next_pos)
                .is_tag("blocks_dolphin_jump")
    }

    fn surface_is_clear(
        level: &ServerLevel,
        dolphin_pos: BlockPos,
        step_x: i32,
        step_z: i32,
        current_step: i32,
    ) -> bool {
        level
            .get_block_state(dolphin_pos.offset(step_x * current_step, 1, step_z * current_step))
            .is_air()
            && level
                .get_block_state(dolphin_pos.offset(
                    step_x * current_step,
                    2,
                    step_z * current_step,
                ))
                .is_air()
    }
}

impl Goal for DolphinJumpGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.base_mut().random.next_int_bounded(self.interval) != 0 {
            return false;
        }
        let motion = mob.get_direction();
        let (step_x, step_z) = (motion.get_step_x(), motion.get_step_z());
        let dolphin_pos = mob.block_position();
        for i in STEPS_TO_CHECK {
            if !Self::water_is_clear(level, dolphin_pos, step_x, step_z, i)
                || !Self::surface_is_clear(level, dolphin_pos, step_x, step_z, i)
            {
                return false;
            }
        }
        true
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        let yd = mob.base().delta_movement().y;
        let x_rot = mob.base().x_rot();
        (!(yd * yd < f64::from(0.03f32))
            || x_rot != 0.0
            || !(x_rot.abs() < 10.0)
            || !mob.is_in_water())
            && !mob.base().on_ground()
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let direction = mob.get_direction();
        let dm = mob.base().delta_movement().add(Vec3::new(
            f64::from(direction.get_step_x()) * 0.6,
            0.7,
            f64::from(direction.get_step_z()) * 0.6,
        ));
        mob.base_mut().set_delta_movement(dm);
        navigation::stop(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        mob.set_x_rot(0.0);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let already_breached = self.breached;
        if !already_breached {
            let fluid_state = level.get_fluid_state(mob.block_position());
            self.breached = fluid_state.is_tag("water");
        }
        if self.breached && !already_breached {
            mob.play_sound(level, mc_data::SoundEvent::ENTITY_DOLPHIN_JUMP, 1.0, 1.0);
        }
        let movement = mob.base().delta_movement();
        let x_rot = mob.base().x_rot();
        if movement.y * movement.y < f64::from(0.03f32) && x_rot != 0.0 {
            mob.set_x_rot(mth::rot_lerp_f32(0.2, x_rot, 0.0));
        } else if movement.length() > f64::from(1.0E-5f32) {
            let horizontal_distance = movement.horizontal_distance();
            let rotation = mth::atan2(-movement.y, horizontal_distance)
                * f64::from(mc_core::math::mth::RAD_TO_DEG);
            mob.set_x_rot(rotation as f32);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}
