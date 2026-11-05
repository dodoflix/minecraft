//! `net.minecraft.util` helpers shared by the game slices (`Util`, `TimeUtil`, DFU-style NBT codec readers,
//! `TimerQueue`). Other slices add their own small helpers here.
pub mod block_util;
pub mod codec;
pub mod game_modes;
pub mod file_util;
pub mod java_format;
pub mod java_hash_order;
pub mod java_uuid_hash_set;
pub mod long_open_hash_set;
pub mod profiling;
pub mod special_dates;
pub mod object_open_hash_map;
pub mod tick_throttler;
pub mod time_util;
pub mod timer_queue;
#[allow(clippy::module_inception)]
pub mod util;
pub mod weighted_list;
