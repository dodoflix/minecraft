//! `net.minecraft.util` helpers shared by the game slices (`Util`, `TimeUtil`, DFU-style NBT codec readers,
//! `TimerQueue`). Other slices add their own small helpers here.
pub mod codec;
pub mod game_modes;
pub mod file_util;
pub mod java_format;
pub mod java_hash_order;
pub mod long_open_hash_set;
pub mod profiling;
pub mod object_open_hash_map;
pub mod tick_throttler;
pub mod time_util;
pub mod timer_queue;
#[allow(clippy::module_inception)]
pub mod util;
