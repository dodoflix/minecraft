//! Items (`Item` ids, class, block form, default data components).

use crate::ids::Block;
pub use crate::ids::Item;

#[derive(Debug)]
pub struct ItemData {
    pub name: &'static str,
    /// Simple name of the vanilla Java class (`Item`, `BlockItem`, `BucketItem`, ...).
    pub class: &'static str,
    pub classes: &'static [&'static str],
    pub description_id: &'static str,
    /// `getDefaultMaxStackSize()` (the `minecraft:max_stack_size` default component).
    pub max_stack_size: u8,
    /// `BlockItem.getBlock()`.
    pub block: Option<Block>,
    /// `getCraftingRemainder().item()`.
    pub crafting_remainder: Option<Item>,
    pub required_features: &'static [&'static str],
    /// The item's default `DataComponentMap` encoded as a `DataComponentPatch` with the vanilla codec
    /// (`{"minecraft:max_stack_size": 64, ...}`), as in the data generator's components report. Object keys are sorted.
    pub components_json: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/items.rs"));

impl Item {
    pub fn data(self) -> &'static ItemData {
        &ITEM_DATA[self.0 as usize]
    }

    /// Default components parsed from [`ItemData::components_json`].
    pub fn default_components(self) -> serde_json::Value {
        serde_json::from_str(self.data().components_json).expect("components_json is valid JSON")
    }

    /// `BlockItem.getBlock()`.
    pub fn block(self) -> Option<Block> {
        self.data().block
    }

    pub fn max_stack_size(self) -> u8 {
        self.data().max_stack_size
    }
}
