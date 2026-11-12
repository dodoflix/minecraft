//! `MoveControl`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob;
use crate::world::level::BlockGetter;
use crate::world::level::block::state::BlockStateExt;
use mc_core::direction::Axis;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::Attribute;
use std::any::Any;

pub const MIN_SPEED: f32 = 5.0E-4;
pub const MIN_SPEED_SQR: f32 = 2.5000003E-7;
pub const MAX_TURN: f32 = 90.0;

/// `MoveControl.Operation`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operation {
    Wait,
    MoveTo,
    Strafe,
    Jumping,
}

/// The fields of `MoveControl` (embedded by subclasses).
#[derive(Debug)]
pub struct MoveControlBase {
    pub wanted_x: f64,
    pub wanted_y: f64,
    pub wanted_z: f64,
    pub speed_modifier: f64,
    pub strafe_forwards: f32,
    pub strafe_right: f32,
    pub operation: Operation,
}

impl Default for MoveControlBase {
    fn default() -> Self {
        Self {
            wanted_x: 0.0,
            wanted_y: 0.0,
            wanted_z: 0.0,
            speed_modifier: 0.0,
            strafe_forwards: 0.0,
            strafe_right: 0.0,
            operation: Operation::Wait,
        }
    }
}

impl MoveControlBase {
    /// `rotlerp(a, b, max)`.
    pub fn rotlerp(&self, a: f32, b: f32, max: f32) -> f32 {
        let mut diff = mth::wrap_degrees_f32(b - a);
        if diff > max {
            diff = max;
        }
        if diff < -max {
            diff = -max;
        }
        let mut result = a + diff;
        if result < 0.0 {
            result += 360.0;
        } else if result > 360.0 {
            result -= 360.0;
        }
        result
    }
}

/// `MoveControl` (subclasses: `FlyingMoveControl`, `SmoothSwimmingMoveControl`, per-mob controls).
pub trait MoveControl: Any {
    fn base(&self) -> &MoveControlBase;
    fn base_mut(&mut self) -> &mut MoveControlBase;

    fn has_wanted(&self) -> bool {
        self.base().operation == Operation::MoveTo
    }

    fn get_speed_modifier(&self) -> f64 {
        self.base().speed_modifier
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        let b = self.base_mut();
        b.wanted_x = x;
        b.wanted_y = y;
        b.wanted_z = z;
        b.speed_modifier = speed_modifier;
        if b.operation != Operation::Jumping {
            b.operation = Operation::MoveTo;
        }
    }

    /// `setWantedPosition` of a control whose override reads the mob (`Rabbit.RabbitMoveControl` checks `isInWater()`); the
    /// control is taken out of the mob while this runs, see [`mob::set_wanted_position`].
    fn set_wanted_position_for(
        &mut self,
        _mob: &dyn Entity,
        x: f64,
        y: f64,
        z: f64,
        speed_modifier: f64,
    ) {
        self.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forwards: f32, right: f32) {
        let b = self.base_mut();
        b.operation = Operation::Strafe;
        b.strafe_forwards = forwards;
        b.strafe_right = right;
        b.speed_modifier = 0.25;
    }

    fn get_wanted_x(&self) -> f64 {
        self.base().wanted_x
    }

    fn get_wanted_y(&self) -> f64 {
        self.base().wanted_y
    }

    fn get_wanted_z(&self) -> f64 {
        self.base().wanted_z
    }

    fn set_wait(&mut self) {
        self.base_mut().operation = Operation::Wait;
    }

    /// `tick()`.
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        base_tick(self.base_mut(), mob, level);
    }
}

/// `isWalkable(dx, dz)`.
fn is_walkable(mob: &mut dyn Entity, level: &mut ServerLevel, dx: f32, dz: f32) -> bool {
    let pos = BlockPos::containing(
        mob.base().x() + f64::from(dx),
        f64::from(mob.block_position().y()),
        mob.base().z() + f64::from(dz),
    );
    mob::navigation::is_walkable_at(mob, level, pos)
}

/// `MoveControl.tick()`.
pub fn base_tick(b: &mut MoveControlBase, mob: &mut dyn Entity, level: &mut ServerLevel) {
    match b.operation {
        Operation::Strafe => {
            let speed = mob::attribute_value(mob, Attribute::MOVEMENT_SPEED) as f32;
            let speed_modified = b.speed_modifier as f32 * speed;
            let mut xa = b.strafe_forwards;
            let mut za = b.strafe_right;
            let mut dist = mth::sqrt(xa * xa + za * za);
            if dist < 1.0 {
                dist = 1.0;
            }
            dist = speed_modified / dist;
            xa *= dist;
            za *= dist;
            let y_rot = mob.base().y_rot();
            let sin = mth::sin(f64::from(y_rot * (std::f64::consts::PI / 180.0) as f32));
            let cos = mth::cos(f64::from(y_rot * (std::f64::consts::PI / 180.0) as f32));
            let dx = xa * cos - za * sin;
            let dz = za * cos + xa * sin;
            if !is_walkable(mob, level, dx, dz) {
                b.strafe_forwards = 1.0;
                b.strafe_right = 0.0;
            }
            mob.set_speed(speed_modified);
            mob::set_zza(mob, b.strafe_forwards);
            mob::set_xxa(mob, b.strafe_right);
            b.operation = Operation::Wait;
        }
        Operation::MoveTo => {
            b.operation = Operation::Wait;
            let xd = b.wanted_x - mob.base().x();
            let zd = b.wanted_z - mob.base().z();
            let yd = b.wanted_y - mob.base().y();
            let dd = xd * xd + yd * yd + zd * zd;
            if dd < f64::from(2.5000003E-7f32) {
                mob::set_zza(mob, 0.0);
                return;
            }
            let y_rot_d =
                (mth::atan2(zd, xd) * f64::from(mc_core::math::mth::RAD_TO_DEG)) as f32 - 90.0;
            let new_y_rot = b.rotlerp(mob.base().y_rot(), y_rot_d, 90.0);
            mob.set_y_rot(new_y_rot);
            mob.set_speed(
                (b.speed_modifier * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED)) as f32,
            );
            let pos = mob.block_position();
            let block_state = level.get_block_state(pos);
            let shape = block_state.get_collision_shape(&*level, pos);
            if yd > f64::from(mob.max_up_step())
                && xd * xd + zd * zd < f64::from(1.0f32.max(mob.base().get_bb_width()))
                || !shape.is_empty()
                    && mob.base().y() < shape.max(Axis::Y) + f64::from(pos.y())
                    && !block_state.block().is_in_tag("doors")
                    && !block_state.block().is_in_tag("fences")
            {
                mob::jump_control_jump(mob);
                b.operation = Operation::Jumping;
            }
        }
        Operation::Jumping => {
            mob.set_speed(
                (b.speed_modifier * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED)) as f32,
            );
            if mob.base().on_ground() || mob.is_in_liquid() && mob.is_affected_by_fluids() {
                b.operation = Operation::Wait;
            }
        }
        Operation::Wait => mob::set_zza(mob, 0.0),
    }
}

/// The plain vanilla `MoveControl`.
#[derive(Default)]
pub struct StandardMoveControl {
    base: MoveControlBase,
}

impl MoveControl for StandardMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }
}
