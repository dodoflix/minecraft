//! `net.minecraft.world.entity.animal.chicken`.

pub mod chicken;
pub mod chicken_sound_variant;
pub mod chicken_variant;

pub use chicken::Chicken;
pub use chicken_sound_variant::{ChickenSoundSet, ChickenSoundVariant};
pub use chicken_variant::ChickenVariant;
