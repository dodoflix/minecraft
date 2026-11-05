//! `ItemCost`: what a trade asks for (an item, a count and the exact component values the offered stack must carry).

use mc_data::Item;
use mc_dfu::extra::{POSITIVE_INT, optional_always_present_field_of};
use mc_dfu::{Codec, record};
use mc_protocol::components::holder::registry_by_name_codec;
use mc_protocol::components::{DataComponentExactPredicate, ItemStack};
use mc_protocol::packets::play::ItemCost as WireItemCost;

/// `ItemCost` (the cached `itemStack` is what `getBaseCostA`/`getCostB` hand out).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemCost {
    item: Item,
    count: i32,
    components: DataComponentExactPredicate,
    item_stack: ItemStack,
}

/// The codec's fields before the cached stack is built.
#[derive(Clone)]
struct Fields {
    item: Item,
    count: i32,
    components: DataComponentExactPredicate,
}

impl ItemCost {
    /// `new ItemCost(item, count, components)`.
    pub fn new(item: Item, count: i32, components: DataComponentExactPredicate) -> ItemCost {
        let item_stack = ItemStack::with_patch(item, count, &components.as_patch());
        ItemCost { item, count, components, item_stack }
    }

    /// `new ItemCost(item, count)`.
    pub fn of(item: Item, count: i32) -> ItemCost {
        ItemCost::new(item, count, DataComponentExactPredicate::empty())
    }

    pub fn item(&self) -> Item {
        self.item
    }

    pub fn count(&self) -> i32 {
        self.count
    }

    pub fn components(&self) -> &DataComponentExactPredicate {
        &self.components
    }

    /// `itemStack()`.
    pub fn item_stack(&self) -> &ItemStack {
        &self.item_stack
    }

    /// `test(itemStack)`: the item matches and the stack carries every expected component value.
    pub fn test(&self, stack: &ItemStack) -> bool {
        stack.is(self.item) && self.components.test(stack)
    }

    /// `ItemCost.CODEC`.
    pub fn codec() -> Codec<ItemCost> {
        record!(Fields {
            item: registry_by_name_codec::<Item>().field_of("id"),
            count: optional_always_present_field_of(&POSITIVE_INT, "count", 1),
            components: DataComponentExactPredicate::codec().optional_field_of_default("components", DataComponentExactPredicate::empty()),
        })
        .xmap(
            |f: Fields| ItemCost::new(f.item, f.count, f.components),
            |c: &ItemCost| Fields { item: c.item, count: c.count, components: c.components.clone() },
        )
        .codec()
    }

    /// The `ItemCost.STREAM_CODEC` value (the encoding lives in `mc-protocol`).
    pub fn to_wire(&self) -> WireItemCost {
        WireItemCost { item: self.item, count: self.count, components: self.components.clone() }
    }

    pub fn from_wire(wire: WireItemCost) -> ItemCost {
        ItemCost::new(wire.item, wire.count, wire.components)
    }
}
