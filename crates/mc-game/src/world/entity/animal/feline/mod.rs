//! `net.minecraft.world.entity.animal.feline`.

pub mod cat;
pub mod cat_sound_variant;
pub mod cat_variant;
pub mod ocelot;

pub use cat::Cat;
pub use cat_sound_variant::{CatSoundSet, CatSoundVariant};
pub use cat_variant::CatVariant;
pub use ocelot::Ocelot;

use crate::world::entity::Entity;
use mc_data::enums::Pose;

pub const CROUCH_SPEED_MOD: f64 = 0.6;
pub const WALK_SPEED_MOD: f64 = 0.8;
pub const SPRINT_SPEED_MOD: f64 = 1.33;

/// `Cat.customServerAiStep` / `Ocelot.customServerAiStep` (identical; neither calls `Animal`'s).
pub fn custom_server_ai_step(e: &mut dyn Entity) {
    let wanted = e.mob().and_then(|m| m.move_control.as_deref()).filter(|c| c.has_wanted()).map(|c| c.get_speed_modifier());
    match wanted {
        Some(speed) if speed == CROUCH_SPEED_MOD => {
            e.set_pose(Pose::Crouching);
            e.set_sprinting(false);
        }
        Some(speed) if speed == SPRINT_SPEED_MOD => {
            e.set_pose(Pose::Standing);
            e.set_sprinting(true);
        }
        _ => {
            e.set_pose(Pose::Standing);
            e.set_sprinting(false);
        }
    }
}
