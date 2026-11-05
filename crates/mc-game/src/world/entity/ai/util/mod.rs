//! `net.minecraft.world.entity.ai.util`: random target positions for goals.
//!
//! Every function takes `(mob, level)` like the goals do; the position suppliers of vanilla are closures here and
//! receive `(mob, level)` as well because they consume the mob's RNG.

pub mod air_and_water_random_pos;
pub mod air_random_pos;
pub mod default_random_pos;
pub mod goal_utils;
pub mod hover_random_pos;
pub mod land_random_pos;
pub mod random_pos;
