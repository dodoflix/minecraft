//! The concrete dispense behaviors of `net.minecraft.core.dispenser` and `DispenseItemBehavior.bootStrap()`.
//!
//! Not implemented: the `bootStrap()` entries other than armor stands, boats, buckets, bone meal, honeycomb, chests,
//! projectiles that are `ProjectileItem`s, water bottles, shulker boxes, TNT and flint and steel (eggs, thrown potions,
//! minecarts and more) have no behavior, so those items are dropped like any other item. Equipment, spawn eggs and
//! sulfur cube swallowing come from the component-based defaults.

mod armor_stand;
mod boat;
mod bone_meal;
mod bucket;
mod chest;
mod equipment;
mod flint_and_steel;
mod honeycomb;
mod potion;
mod projectile;
mod shulker_box;
mod spawn_egg;
mod sulfur_cube;
mod tnt;

pub use equipment::{EQUIPMENT, dispense_equipment};
pub use spawn_egg::SPAWN_EGG;
pub use sulfur_cube::{SULFUR_CUBE, dispense_block};
