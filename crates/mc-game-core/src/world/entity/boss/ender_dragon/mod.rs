//! `net.minecraft.world.entity.boss.enderdragon`: the ender dragon and its hitboxes.

mod ender_dragon;
mod flight_history;
mod part;
mod phase;

pub use ender_dragon::{DATA_PHASE, EnderDragon};
pub use flight_history::{DragonFlightHistory, Sample};
pub use part::{BODY, DRAGON_PARTS, EnderDragonPart, HEAD, NECK, TAIL_1, WING_1, WING_2};
pub use phase::EnderDragonPhase;
