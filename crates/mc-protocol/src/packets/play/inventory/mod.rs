//! Play packets: group `inventory` (containers, recipe book/displays, maps, trading, books/signs; see packets/play/mod.rs).
mod container;
mod display;
mod map;
mod recipe;
mod slots;
mod untrusted_text;

pub use container::*;
pub use display::*;
pub use map::*;
pub use recipe::*;
pub use slots::*;
