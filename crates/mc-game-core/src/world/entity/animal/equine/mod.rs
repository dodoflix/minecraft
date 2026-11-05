//! `net.minecraft.world.entity.animal.equine`: horses, donkeys, mules, llamas, skeleton/zombie horses.

pub mod abstract_horse;

pub use abstract_horse::{AbstractHorse, AbstractHorseData};

use crate::world::item::ItemStack;
use mc_data::Item;

/// `itemStack.is(ItemTags.X)`.
pub fn item_in_tag(stack: &ItemStack, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}
