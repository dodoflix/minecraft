//! `ItemInput`: an item plus the component patch written in the command.

use crate::commands::message::message;
use crate::translatable;
use mc_brigadier::{
    CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType,
};
use mc_data::Item;
use mc_protocol::components::{DataComponentPatch, ItemStack};
use std::sync::LazyLock;

static ERROR_STACK_TOO_BIG: LazyLock<Dynamic2CommandExceptionType<String, i32>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|item: String, count: i32| {
            message(translatable!("arguments.item.overstacked", item, count))
        })
    });
static ERROR_MALFORMED_ITEM: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|error: String| {
        message(translatable!("arguments.item.malformed", error))
    })
});

#[derive(Clone, Debug)]
pub struct ItemInput {
    pub item: Item,
    pub components: DataComponentPatch,
}

impl ItemInput {
    /// `createItemStack(count)`.
    pub fn create_item_stack(&self, count: i32) -> Result<ItemStack, CommandSyntaxException> {
        let result = ItemStack::with_patch(self.item, count, &self.components);
        let max = result.get_max_stack_size();
        if count > max {
            return Err(ERROR_STACK_TOO_BIG.create(self.item.name().to_owned(), max));
        }
        ItemStack::validate_strict(result)
            .into_result()
            .map_err(|e| ERROR_MALFORMED_ITEM.create(e))
    }
}
