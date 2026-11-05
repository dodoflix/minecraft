//! `ApplyEntityImpulse.apply`: the push is the entity's look rotation (a JOML float `Quaternionf`) applied to `direction`.

use mc_core::phys::Vec3;

use super::ApplyEntityImpulse;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::item::enchantment::actors::Actors;

const DEG_TO_RAD: f32 = (std::f64::consts::PI / 180.0) as f32;

/// `org.joml.Math.cosFromSin(sin, angle)` (non-fast-math branch, `float`).
fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    let cos = (1.0f32 - sin * sin).sqrt();
    let a = angle + (std::f64::consts::PI / 2.0) as f32;
    let two_pi = (std::f64::consts::PI * 2.0) as f32;
    let mut b = a - ((a / two_pi) as i32) as f32 * two_pi;
    if (b as f64) < 0.0 {
        b += two_pi;
    }
    if b >= std::f64::consts::PI as f32 { -cos } else { cos }
}

/// `Entity.calculateViewQuaternion(xRot, yRot)` = `new Quaternionf().rotationY(-yRot * DEG).rotateX(xRot * DEG)`; `[x, y, z, w]`.
fn view_quaternion(x_rot: f32, y_rot: f32) -> [f32; 4] {
    let angle = -y_rot * DEG_TO_RAD;
    let sin = (angle * 0.5f32).sin_java();
    let cos = cos_from_sin(sin, angle * 0.5f32);
    let (x, y, z, w) = (0.0f32, sin, 0.0f32, cos);
    let angle = x_rot * DEG_TO_RAD;
    let sin = (angle * 0.5f32).sin_java();
    let cos = cos_from_sin(sin, angle * 0.5f32);
    [w * sin + x * cos, y * cos + z * sin, z * cos - y * sin, w * cos - x * sin]
}

trait SinJava {
    fn sin_java(self) -> f32;
}
impl SinJava for f32 {
    /// `(float) java.lang.Math.sin(rad)`.
    fn sin_java(self) -> f32 {
        (self as f64).sin() as f32
    }
}

/// `Quaternionf.transform(x, y, z, Vector3d)` (no `Math.fma`: JOML's `Runtime.HAS_Math_fma` is off by default).
fn transform(q: [f32; 4], x: f64, y: f64, z: f64) -> Vec3 {
    let [qx, qy, qz, qw] = q;
    let xx = qx * qx;
    let yy = qy * qy;
    let zz = qz * qz;
    let ww = qw * qw;
    let xy = qx * qy;
    let xz = qx * qz;
    let yz = qy * qz;
    let xw = qx * qw;
    let zw = qz * qw;
    let yw = qy * qw;
    let k = 1.0f32 / (xx + yy + zz + ww);
    let fma = |a: f64, b: f64, c: f64| a * b + c;
    Vec3::new(
        fma(((xx - yy - zz + ww) * k) as f64, x, fma((2.0f32 * (xy - zw) * k) as f64, y, (2.0f32 * (xz + yw) * k) as f64 * z)),
        fma((2.0f32 * (xy + zw) * k) as f64, x, fma(((yy - xx - zz + ww) * k) as f64, y, (2.0f32 * (yz - xw) * k) as f64 * z)),
        fma((2.0f32 * (xz - yw) * k) as f64, x, fma((2.0f32 * (yz + xw) * k) as f64, y, ((zz - xx - yy + ww) * k) as f64 * z)),
    )
}

pub fn apply_entity_impulse(e: &ApplyEntityImpulse, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, entity: EntityId) {
    let magnitude = e.magnitude.calculate(enchantment_level) as f64;
    let push = actors.with(level, entity, |_, t| {
        let look = view_quaternion(t.base().x_rot(), t.base().y_rot());
        let d = transform(look, e.direction.x, e.direction.y, e.direction.z);
        // `.mul(coordinateScale.x, .y, .z).mul(magnitude)`
        let d = Vec3::new(d.x * e.coordinate_scale.x, d.y * e.coordinate_scale.y, d.z * e.coordinate_scale.z);
        let d = Vec3::new(d.x * magnitude, d.y * magnitude, d.z * magnitude);
        t.base_mut().add_delta_movement(d);
        let is_player = t.is_player();
        t.base_mut().sync_velocity = !is_player;
        if let Some(living) = t.living_mut() {
            // `applyPostImpulseGraceTime(10)`
            living.current_impulse_context_reset_grace_time = living.current_impulse_context_reset_grace_time.max(10);
        }
        is_player.then(|| (t.id(), t.base().delta_movement()))
    });
    if let Some(Some((id, movement))) = push {
        level.send_to_player(id, &mc_protocol::packets::play::ClientboundSetEntityMotionPacket { id, movement: mc_protocol::codec::LpVec3(movement) });
    }
}
