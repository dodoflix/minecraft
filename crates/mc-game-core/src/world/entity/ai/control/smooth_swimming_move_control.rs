//! `SmoothSwimmingMoveControl`.

use super::move_control::{MoveControl, MoveControlBase, Operation};
use super::rotate_towards;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::Attribute;

const FULL_SPEED_TURN_THRESHOLD: f32 = 10.0;
const STOP_TURN_THRESHOLD: f32 = 60.0;

pub struct SmoothSwimmingMoveControl {
    base: MoveControlBase,
    max_turn_x: i32,
    max_turn_y: i32,
    in_water_speed_modifier: f32,
    outside_water_speed_modifier: f32,
    apply_gravity: bool,
}

impl SmoothSwimmingMoveControl {
    pub fn new(
        max_turn_x: i32,
        max_turn_y: i32,
        in_water_speed_modifier: f32,
        outside_water_speed_modifier: f32,
        apply_gravity: bool,
    ) -> Self {
        Self {
            base: MoveControlBase::default(),
            max_turn_x,
            max_turn_y,
            in_water_speed_modifier,
            outside_water_speed_modifier,
            apply_gravity,
        }
    }
}

fn turning_speed_factor(left_to_turn: f32) -> f32 {
    1.0 - mth::clamp_f32(
        (left_to_turn - FULL_SPEED_TURN_THRESHOLD)
            / (STOP_TURN_THRESHOLD - FULL_SPEED_TURN_THRESHOLD),
        0.0,
        1.0,
    )
}

impl MoveControl for SmoothSwimmingMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.apply_gravity && mob.is_in_water() {
            let dm = mob.base().delta_movement().add(Vec3::new(0.0, 0.005, 0.0));
            mob.base_mut().set_delta_movement(dm);
        }
        let b = &mut self.base;
        if b.operation == Operation::MoveTo && !mob::navigation::is_done(mob, level) {
            let xd = b.wanted_x - mob.base().x();
            let yd = b.wanted_y - mob.base().y();
            let zd = b.wanted_z - mob.base().z();
            let dd = xd * xd + yd * yd + zd * zd;
            if dd < f64::from(2.5000003E-7f32) {
                mob::set_zza(mob, 0.0);
            } else {
                let y_rot_d = (mth::atan2(zd, xd) * f64::from(180.0f32)
                    / f64::from(std::f32::consts::PI)) as f32
                    - 90.0;
                let new_y_rot = b.rotlerp(mob.base().y_rot(), y_rot_d, self.max_turn_y as f32);
                mob.set_y_rot(new_y_rot);
                {
                    let l = mob.living_mut().expect("mob");
                    l.y_body_rot = new_y_rot;
                    l.y_head_rot = new_y_rot;
                }
                let speed = (b.speed_modifier
                    * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED))
                    as f32;
                if mob.is_in_water() {
                    mob.set_speed(speed * self.in_water_speed_modifier);
                    let sqrt = (xd * xd + zd * zd).sqrt();
                    if yd.abs() > f64::from(1.0E-5f32) || sqrt.abs() > f64::from(1.0E-5f32) {
                        let mut x_rot_d = -((mth::atan2(yd, sqrt) * f64::from(180.0f32)
                            / f64::from(std::f32::consts::PI))
                            as f32);
                        x_rot_d = mth::clamp_f32(
                            mth::wrap_degrees_f32(x_rot_d),
                            -(self.max_turn_x as f32),
                            self.max_turn_x as f32,
                        );
                        let new_x_rot = rotate_towards(mob.base().x_rot(), x_rot_d, 5.0);
                        mob.set_x_rot(new_x_rot);
                    }
                    let x_rot = mob.base().x_rot();
                    let cos = mth::cos(f64::from(x_rot * (std::f64::consts::PI / 180.0) as f32));
                    let sin = mth::sin(f64::from(x_rot * (std::f64::consts::PI / 180.0) as f32));
                    let l = mob.living_mut().expect("mob");
                    l.zza = cos * speed;
                    l.yya = -sin * speed;
                } else {
                    let left_to_turn = mth::wrap_degrees_f32(mob.base().y_rot() - y_rot_d).abs();
                    let factor = turning_speed_factor(left_to_turn);
                    mob.set_speed(speed * self.outside_water_speed_modifier * factor);
                }
            }
        } else {
            mob.set_speed(0.0);
            mob::set_xxa(mob, 0.0);
            mob::set_yya(mob, 0.0);
            mob::set_zza(mob, 0.0);
        }
    }
}
