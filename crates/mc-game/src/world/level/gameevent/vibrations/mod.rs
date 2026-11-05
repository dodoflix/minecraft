//! `net.minecraft.world.level.gameevent.vibrations`: the persistent data of the vibration system (sculk sensors and
//! shriekers save it as their `listener` field). The listener/tick logic belongs to the game event slice.

mod vibration_info;
mod vibration_selector;
mod vibration_system;

pub use vibration_info::VibrationInfo;
pub use vibration_selector::VibrationSelector;
pub use vibration_system::VibrationData;
