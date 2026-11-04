//! `net.minecraft.util` helpers shared by the game slices (`Util`, `TimeUtil`, DFU-style NBT codec readers,
//! `TimerQueue`). Other slices add their own small helpers here.
pub mod codec;
pub mod java_format;
pub mod long_open_hash_set;
pub mod tick_throttler;
pub mod time_util;
pub mod timer_queue;
#[allow(clippy::module_inception)]
pub mod util;
