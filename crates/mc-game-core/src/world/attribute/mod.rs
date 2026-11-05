//! `net.minecraft.world.attribute` + `world.timeline`: the environment attribute system (dimension constants, biome
//! layers, timeline tracks driven by world clocks, weather layers) for every attribute type.
pub mod attributes;
pub mod easing;
pub mod system;
pub mod value;

pub use easing::EasingType;
pub use system::{AttributeEnv, EnvironmentAttributeSystem, Timeline};
pub use value::{AttrValue, BedRule, BedRuleRule, MobSpawnSettings, TriState};
