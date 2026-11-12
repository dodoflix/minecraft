//! `FlyingMoveControl`.

use super::move_control::{MoveControl, MoveControlBase, Operation};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob;
use mc_core::math::mth;
use mc_data::Attribute;

pub struct FlyingMoveControl {
    base: MoveControlBase,
    max_turn: i32,
    hovers_in_place: bool,
}

impl FlyingMoveControl {
    pub fn new(max_turn: i32, hovers_in_place: bool) -> Self {
        Self {
            base: MoveControlBase::default(),
            max_turn,
            hovers_in_place,
        }
    }
}

impl MoveControl for FlyingMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let b = &mut self.base;
        if b.operation == Operation::MoveTo {
            b.operation = Operation::Wait;
            mob.base_mut().set_no_gravity(true);
            let xd = b.wanted_x - mob.base().x();
            let yd = b.wanted_y - mob.base().y();
            let zd = b.wanted_z - mob.base().z();
            let dd = xd * xd + yd * yd + zd * zd;
            if dd < f64::from(2.5000003E-7f32) {
                mob::set_yya(mob, 0.0);
                mob::set_zza(mob, 0.0);
                return;
            }
            let y_rot_d =
                (mth::atan2(zd, xd) * f64::from(mc_core::math::mth::RAD_TO_DEG)) as f32 - 90.0;
            let new_y_rot = b.rotlerp(mob.base().y_rot(), y_rot_d, 90.0);
            mob.set_y_rot(new_y_rot);
            let speed = if mob.base().on_ground() {
                (b.speed_modifier * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED)) as f32
            } else {
                (b.speed_modifier * mob::attribute_value(mob, Attribute::FLYING_SPEED)) as f32
            };
            mob.set_speed(speed);
            let sd = (xd * xd + zd * zd).sqrt();
            if yd.abs() > f64::from(1.0E-5f32) || sd.abs() > f64::from(1.0E-5f32) {
                let x_rot_d =
                    (-(mth::atan2(yd, sd) * f64::from(mc_core::math::mth::RAD_TO_DEG))) as f32;
                let new_x_rot = b.rotlerp(mob.base().x_rot(), x_rot_d, self.max_turn as f32);
                mob.set_x_rot(new_x_rot);
                mob::set_yya(mob, if yd > 0.0 { speed } else { -speed });
            }
        } else {
            if !self.hovers_in_place {
                mob.base_mut().set_no_gravity(false);
            }
            mob::set_yya(mob, 0.0);
            mob::set_zza(mob, 0.0);
        }
    }
}
