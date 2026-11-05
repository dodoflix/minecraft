//! `net.minecraft.world.entity.animal.wolf`.

pub mod wolf;
pub mod wolf_sound_variant;
pub mod wolf_variant;

pub use wolf::Wolf;
pub use wolf_sound_variant::{WolfSoundSet, WolfSoundVariant};
pub use wolf_variant::{WolfVariant, WolfVariantAssetInfo};
