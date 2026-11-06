//! The concrete dispense behaviors of `net.minecraft.core.dispenser` and `DispenseItemBehavior.bootStrap()`.
//!
//! Not implemented: only the behaviors of armor stands, boats, water bottles, shulker boxes and TNT exist. Every other
//! `bootStrap()` entry (arrows, snowballs, eggs, fire charges, buckets, bone meal, flint and steel, spawn eggs, minecarts,
//! glass bottles, honeycomb, shears and more) is missing, so those items are dropped like any other item.

mod armor_stand;
mod boat;
mod potion;
mod shulker_box;
mod tnt;
