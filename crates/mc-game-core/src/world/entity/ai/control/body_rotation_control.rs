//! `BodyRotationControl`.

use crate::world::entity::Entity;
use mc_core::math::mth;
use std::any::Any;

const HEAD_STABLE_ANGLE: f32 = 15.0;
const DELAY_UNTIL_STARTING_TO_FACE_FORWARD: i32 = 10;
const HOW_LONG_IT_TAKES_TO_FACE_FORWARD: i32 = 10;

#[derive(Default, Debug)]
pub struct BodyRotationControlBase {
    pub head_stable_time: i32,
    pub last_stable_y_head_rot: f32,
}

/// `BodyRotationControl` (subclasses: shulker, phantom, guardian, ... via `createBodyControl()`).
pub trait BodyRotationControl: Any {
    fn base(&self) -> &BodyRotationControlBase;
    fn base_mut(&mut self) -> &mut BodyRotationControlBase;

    /// `clientTick()` (called by `Mob.tickHeadTurn` on the server too).
    fn client_tick(&mut self, mob: &mut dyn Entity) {
        base_client_tick(self.base_mut(), mob);
    }
}

fn is_moving(mob: &dyn Entity) -> bool {
    let xd = mob.base().x() - mob.base().xo;
    let zd = mob.base().z() - mob.base().zo;
    xd * xd + zd * zd > f64::from(2.5000003E-7f32)
}

/// `BodyRotationControl.clientTick()`.
pub fn base_client_tick(b: &mut BodyRotationControlBase, mob: &mut dyn Entity) {
    let max_head_y_rot = mob.get_max_head_y_rot() as f32;
    let carrying_mob = mob.mob().is_some_and(|m| m.first_passenger_is_mob);
    if is_moving(mob) {
        let y_rot = mob.base().y_rot();
        let l = mob.living_mut().expect("mob");
        l.y_body_rot = y_rot;
        // rotateHeadIfNecessary
        l.y_head_rot = mth::rotate_if_necessary(l.y_head_rot, l.y_body_rot, max_head_y_rot);
        b.last_stable_y_head_rot = l.y_head_rot;
        b.head_stable_time = 0;
    } else if !carrying_mob {
        let l = mob.living_mut().expect("mob");
        if (l.y_head_rot - b.last_stable_y_head_rot).abs() > HEAD_STABLE_ANGLE {
            b.head_stable_time = 0;
            b.last_stable_y_head_rot = l.y_head_rot;
            // rotateBodyIfNecessary
            l.y_body_rot = mth::rotate_if_necessary(l.y_body_rot, l.y_head_rot, max_head_y_rot);
        } else {
            b.head_stable_time += 1;
            if b.head_stable_time > DELAY_UNTIL_STARTING_TO_FACE_FORWARD {
                // rotateHeadTowardsFront
                let time_since = b.head_stable_time - DELAY_UNTIL_STARTING_TO_FACE_FORWARD;
                let face_forward_fraction = mth::clamp_f32(time_since as f32 / HOW_LONG_IT_TAKES_TO_FACE_FORWARD as f32, 0.0, 1.0);
                let angle_remaining = max_head_y_rot * (1.0 - face_forward_fraction);
                l.y_body_rot = mth::rotate_if_necessary(l.y_body_rot, l.y_head_rot, angle_remaining);
            }
        }
    }
}

/// The plain vanilla `BodyRotationControl`.
#[derive(Default)]
pub struct StandardBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for StandardBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }
}
