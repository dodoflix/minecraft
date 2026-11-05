//! `net.minecraft.world.level.block.CaveVines` (interface shared by `CaveVinesBlock` and `CaveVinesPlantBlock`).
use crate::world::item::ItemStack;
use mc_data::item::Item;

/// `new ItemStack(Items.GLOW_BERRIES)`.
pub fn glow_berries() -> ItemStack {
    ItemStack::new(Item::GLOW_BERRIES)
}
