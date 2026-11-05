//! `net.minecraft.world.entity.ai`. The goal framework, controls, random-position helpers and targeting conditions
//! live here (MobSpine); memories/brain/behaviors (BrainFramework) and navigation (PathfinderCore) are siblings.

pub mod control;
pub mod goal;
pub mod navigation;
pub mod sensing;
pub mod targeting;
pub mod util;
