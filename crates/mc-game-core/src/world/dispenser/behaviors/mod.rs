//! The concrete dispense behaviors of `net.minecraft.core.dispenser` and `DispenseItemBehavior.bootStrap()`.
//!
//! Not implemented: only the behaviors of armor stands, boats, water bottles, shulker boxes and TNT exist, next to the
//! component-based defaults (equipment, spawn eggs, sulfur cube swallowing). Every other `bootStrap()` entry (arrows,
//! snowballs, eggs, fire charges, buckets, bone meal, flint and steel, minecarts, glass bottles, honeycomb, shears and
//! more) is missing, so those items are dropped like any other item.

mod armor_stand;
mod boat;
mod equipment;
mod potion;
mod shulker_box;
mod spawn_egg;
mod sulfur_cube;
mod tnt;

pub use equipment::{EQUIPMENT, dispense_equipment};
pub use spawn_egg::SPAWN_EGG;
pub use sulfur_cube::{SULFUR_CUBE, dispense_block};
