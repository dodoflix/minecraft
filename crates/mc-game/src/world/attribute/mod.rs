//! `net.minecraft.world.attribute` + `world.timeline`: the environment attribute system (dimension constants, timeline
//! tracks driven by world clocks, weather layers) for the attribute types the server evaluates: `float`, `boolean` and
//! `moon_phase` (sky light level, moon phase, `gameplay/*` rules). Other attribute types (colors, particles, music,
//! bed rules, ...) are client presentation or consumed by slices that decode them from the dimension type directly.
pub mod easing;
pub mod system;

pub use easing::EasingType;
pub use system::{AttrValue, AttributeEnv, EnvironmentAttributeSystem, Timeline};
