//! `LookControl`.

use super::rotate_towards;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob;
use mc_core::math::mth;
use std::any::Any;

/// The fields of `LookControl`.
#[derive(Default, Debug)]
pub struct LookControlBase {
    pub y_max_rot_speed: f32,
    pub x_max_rot_angle: f32,
    pub look_at_cooldown: i32,
    pub wanted_x: f64,
    pub wanted_y: f64,
    pub wanted_z: f64,
}

/// `LookControl` (subclasses: `SmoothSwimmingLookControl`, per-mob controls).
pub trait LookControl: Any {
    fn base(&self) -> &LookControlBase;
    fn base_mut(&mut self) -> &mut LookControlBase;

    /// `setLookAt(x, y, z, yMaxRotSpeed, xMaxRotAngle)`.
    fn set_look_at_full(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        y_max_rot_speed: f32,
        x_max_rot_angle: f32,
    ) {
        let b = self.base_mut();
        b.wanted_x = x;
        b.wanted_y = y;
        b.wanted_z = z;
        b.y_max_rot_speed = y_max_rot_speed;
        b.x_max_rot_angle = x_max_rot_angle;
        b.look_at_cooldown = 2;
    }

    /// `setLookAt(target, yMaxRotSpeed, xMaxRotAngle)`.
    fn set_look_at_entity_full(
        &mut self,
        target: &dyn Entity,
        y_max_rot_speed: f32,
        x_max_rot_angle: f32,
    ) {
        self.set_look_at_full(
            target.base().x(),
            target.base().get_eye_y(),
            target.base().z(),
            y_max_rot_speed,
            x_max_rot_angle,
        );
    }

    fn is_looking_at_target(&self) -> bool {
        self.base().look_at_cooldown > 0
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

    /// `resetXRotOnTick()`.
    fn reset_x_rot_on_tick(&self) -> bool {
        true
    }

    /// `getXRotD()`.
    fn get_x_rot_d(&self, mob: &dyn Entity) -> Option<f32> {
        x_rot_d(self.base(), mob)
    }

    /// `getYRotD()`.
    fn get_y_rot_d(&self, mob: &dyn Entity) -> Option<f32> {
        y_rot_d(self.base(), mob)
    }

    /// `clampHeadRotationToBody()`.
    fn clamp_head_rotation_to_body(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        base_clamp_head_rotation_to_body(mob, level);
    }

    /// `tick()`.
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        default_tick(self, mob, level);
    }
}

/// `LookControl.tick()` (for subclasses that wrap it, like `Bee.BeeLookControl`).
pub fn default_tick<L: LookControl + ?Sized>(
    this: &mut L,
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
) {
    if this.reset_x_rot_on_tick() {
        mob.set_x_rot(0.0);
    }
    if this.base().look_at_cooldown > 0 {
        this.base_mut().look_at_cooldown -= 1;
        if let Some(y_rot_d) = this.get_y_rot_d(mob) {
            let speed = this.base().y_max_rot_speed;
            let l = mob.living_mut().expect("mob");
            l.y_head_rot = rotate_towards(l.y_head_rot, y_rot_d, speed);
        }
        if let Some(x_rot_d) = this.get_x_rot_d(mob) {
            let rot = rotate_towards(mob.base().x_rot(), x_rot_d, this.base().x_max_rot_angle);
            mob.set_x_rot(rot);
        }
    } else {
        let l = mob.living_mut().expect("mob");
        l.y_head_rot = rotate_towards(l.y_head_rot, l.y_body_rot, 10.0);
    }
    this.clamp_head_rotation_to_body(mob, level);
}

/// `LookControl.getXRotD()`.
pub fn x_rot_d(b: &LookControlBase, mob: &dyn Entity) -> Option<f32> {
    let xd = b.wanted_x - mob.base().x();
    let yd = b.wanted_y - mob.base().get_eye_y();
    let zd = b.wanted_z - mob.base().z();
    let sd = (xd * xd + zd * zd).sqrt();
    if !(yd.abs() > f64::from(1.0E-5f32)) && !(sd.abs() > f64::from(1.0E-5f32)) {
        None
    } else {
        Some((-(mth::atan2(yd, sd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI))) as f32)
    }
}

/// `LookControl.getYRotD()`.
pub fn y_rot_d(b: &LookControlBase, mob: &dyn Entity) -> Option<f32> {
    let xd = b.wanted_x - mob.base().x();
    let zd = b.wanted_z - mob.base().z();
    if !(zd.abs() > f64::from(1.0E-5f32)) && !(xd.abs() > f64::from(1.0E-5f32)) {
        None
    } else {
        Some(
            (mth::atan2(zd, xd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI)) as f32
                - 90.0,
        )
    }
}

/// `LookControl.clampHeadRotationToBody()`.
pub fn base_clamp_head_rotation_to_body(mob: &mut dyn Entity, level: &mut ServerLevel) {
    if !mob::navigation::is_done(mob, level) {
        let max = mob.get_max_head_y_rot() as f32;
        let l = mob.living_mut().expect("mob");
        l.y_head_rot = mth::rotate_if_necessary(l.y_head_rot, l.y_body_rot, max);
    }
}

/// The plain vanilla `LookControl`.
#[derive(Default)]
pub struct StandardLookControl {
    base: LookControlBase,
}

impl LookControl for StandardLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }
}
