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

use mc_protocol::components::DyeColor;

/// `DyeColor.getTextureDiffuseColor()` / `getFireworkColor()` in `DyeColor` order.
const DYE_COLORS: [(i32, i32); 16] = [
    (16383998, 15790320),
    (16351261, 15435844),
    (13061821, 12801229),
    (3847130, 6719955),
    (16701501, 14602026),
    (8439583, 4312372),
    (15961002, 14188952),
    (4673362, 4408131),
    (10329495, 11250603),
    (1481884, 2651799),
    (8991416, 8073150),
    (3949738, 2437522),
    (8606770, 5320730),
    (6192150, 3887386),
    (11546150, 11743532),
    (1908001, 1973019),
];

/// `DyeColor.getTextureDiffuseColor()`.
pub fn dye_texture_diffuse_color(dye: DyeColor) -> i32 {
    DYE_COLORS[dye.id() as usize].0
}

/// `DyeColor.getFireworkColor()`.
pub fn dye_firework_color(dye: DyeColor) -> i32 {
    DYE_COLORS[dye.id() as usize].1
}

/// `BannerItem.getColor()` for a banner item (`None` for every other item).
pub fn banner_color(item: mc_data::Item) -> Option<DyeColor> {
    let name = mc_data::registry::by_name("minecraft:item")?.name_of(item.id())?;
    let color = name.strip_prefix("minecraft:")?.strip_suffix("_banner")?;
    DyeColor::ALL.iter().copied().find(|c| c.name() == color)
}

/// The component patch obtained by applying `f` to a fresh stack of `item` (vanilla's `DataComponentPatch.builder()`).
pub fn patch_of(item: mc_data::Item, f: impl FnOnce(&mut ItemStack)) -> DataComponentPatch {
    let mut stack = ItemStack::new(item);
    f(&mut stack);
    stack.get_components_patch()
}
