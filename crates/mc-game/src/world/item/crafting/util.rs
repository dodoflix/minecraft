//! Helpers shared by the recipe kinds (`ItemStackTemplate.apply`, `TransmuteRecipe.createWithOriginalComponents`).

use mc_protocol::components::{DataComponentPatch, ItemStackTemplate};

use crate::world::item::ItemStack;

/// `ItemStackTemplate.apply(count, additionalPatch)`: the stack of `template` with `additional` components underneath the
/// template's own patch, strictly validated (an invalid stack is logged and becomes empty).
pub fn template_apply(template: &ItemStackTemplate, count: i32, additional: &DataComponentPatch) -> ItemStack {
    let mut result = ItemStack::with_patch(template.item, count, additional);
    result.apply_components(&template.components);
    match ItemStack::validate_strict(result).into_result() {
        Ok(stack) => stack,
        Err(error) => {
            log::warn!("Can't create item stack with properties {template:?}, error: {error}");
            ItemStack::empty()
        }
    }
}

/// `TransmuteRecipe.createWithOriginalComponents(target, input, extraCount)`.
pub fn create_with_original_components(target: &ItemStackTemplate, input: &ItemStack, extra_count: i32) -> ItemStack {
    template_apply(target, target.count.wrapping_add(extra_count), &input.get_components_patch())
}
