//! `net.minecraft.world.entity.ai.control`: move/look/jump/body-rotation controls.
//!
//! Each control is a trait object stored in `MobData` (so mob classes can substitute subclasses: `FlyingMoveControl`,
//! `SmoothSwimmingLookControl`, ...). A control embeds its `*Base` state struct; the trait defaults run the vanilla base
//! class behavior, and a subclass overriding `tick` calls the base free function (`move_control::base_tick`) as its
//! `super.tick()`. While a control ticks it is `mem::take`n out of the mob, so it gets `(mob, level)` explicitly.

pub mod body_rotation_control;
pub mod flying_move_control;
pub mod jump_control;
pub mod look_control;
pub mod move_control;
pub mod smooth_swimming_look_control;
pub mod smooth_swimming_move_control;

pub use body_rotation_control::{BodyRotationControl, BodyRotationControlBase};
pub use flying_move_control::FlyingMoveControl;
pub use jump_control::{JumpControl, JumpControlBase};
pub use look_control::{LookControl, LookControlBase};
pub use move_control::{MoveControl, MoveControlBase, Operation};
pub use smooth_swimming_look_control::SmoothSwimmingLookControl;
pub use smooth_swimming_move_control::SmoothSwimmingMoveControl;

use mc_core::math::mth;

/// `Control.rotateTowards(fromAngle, toAngle, maxRot)`.
pub fn rotate_towards(from_angle: f32, to_angle: f32, max_rot: f32) -> f32 {
    let diff = mth::degrees_difference(from_angle, to_angle);
    let diff_clamped = mth::clamp_f32(diff, -max_rot, max_rot);
    from_angle + diff_clamped
}
