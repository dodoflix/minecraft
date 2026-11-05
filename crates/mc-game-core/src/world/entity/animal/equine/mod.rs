//! `net.minecraft.world.entity.animal.equine`: horses, donkeys, mules, llamas, skeleton/zombie horses.

pub mod abstract_chested_horse;
pub mod abstract_horse;
pub mod donkey;
pub mod horse;
pub mod llama;
pub mod llama_goals;
pub mod markings;
pub mod mule;
pub mod skeleton_horse;
pub mod skeleton_trap_goal;
pub mod zombie_horse;

pub use abstract_chested_horse::AbstractChestedHorse;
pub use abstract_horse::{AbstractHorse, AbstractHorseData};
pub use donkey::Donkey;
pub use horse::Horse;
pub use llama::Llama;
pub use markings::Markings;
pub use mule::Mule;
pub use skeleton_horse::SkeletonHorse;
pub use zombie_horse::ZombieHorse;

use crate::world::item::ItemStack;
use mc_data::Item;

/// `itemStack.is(ItemTags.X)`.
pub fn item_in_tag(stack: &ItemStack, tag: &str) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}
