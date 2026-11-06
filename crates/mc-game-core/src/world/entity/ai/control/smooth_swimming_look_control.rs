//! `SmoothSwimmingLookControl`.

use super::look_control::{LookControl, LookControlBase};
use super::rotate_towards;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob;
use mc_core::math::mth;

const HEAD_TILT_X: f32 = 10.0;
const HEAD_TILT_Y: f32 = 20.0;

pub struct SmoothSwimmingLookControl {
    base: LookControlBase,
    max_y_rot_from_center: i32,
}

impl SmoothSwimmingLookControl {
    pub fn new(max_y_rot_from_center: i32) -> Self {
        Self {
            base: LookControlBase::default(),
            max_y_rot_from_center,
        }
    }
}

impl LookControl for SmoothSwimmingLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.base.look_at_cooldown > 0 {
            self.base.look_at_cooldown -= 1;
            if let Some(y_rot_d) = self.get_y_rot_d(mob) {
                let speed = self.base.y_max_rot_speed;
                let l = mob.living_mut().expect("mob");
                l.y_head_rot = rotate_towards(l.y_head_rot, y_rot_d + HEAD_TILT_Y, speed);
            }
            if let Some(x_rot_d) = self.get_x_rot_d(mob) {
                let rot = rotate_towards(
                    mob.base().x_rot(),
                    x_rot_d + HEAD_TILT_X,
                    self.base.x_max_rot_angle,
                );
                mob.set_x_rot(rot);
            }
        } else {
            if mob::navigation::is_done(mob, level) {
                let rot = rotate_towards(mob.base().x_rot(), 0.0, 5.0);
                mob.set_x_rot(rot);
            }
            let speed = self.base.y_max_rot_speed;
            let l = mob.living_mut().expect("mob");
            l.y_head_rot = rotate_towards(l.y_head_rot, l.y_body_rot, speed);
        }
        let l = mob.living_mut().expect("mob");
        let head_diff_body = mth::wrap_degrees_f32(l.y_head_rot - l.y_body_rot);
        if head_diff_body < -(self.max_y_rot_from_center as f32) {
            l.y_body_rot -= 4.0;
        } else if head_diff_body > self.max_y_rot_from_center as f32 {
            l.y_body_rot += 4.0;
        }
    }
}
