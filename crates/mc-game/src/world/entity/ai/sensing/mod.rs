//! `net.minecraft.world.entity.ai.sensing` (the `Sensing` per-tick line-of-sight cache; BrainFramework adds the
//! `Sensor` infrastructure next to it).

pub mod sensing;

pub use sensing::Sensing;
