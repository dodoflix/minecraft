//! `DyeColor` helpers that need game state: `byId` (out of bounds = white) and `getMixedColor(level, c1, c2)`.

use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::world::item::crafting::CraftingInput;
use crate::world::level::level_access::LevelAccessor as _;
use mc_core::random::RandomSource;
use mc_data::{Item, RecipeType};
use mc_protocol::components::{DYE, DyeColor};

/// `DyeColor.byId(id)` (`ByIdMap.OutOfBoundsStrategy.ZERO`).
pub fn by_id(id: i32) -> DyeColor {
    usize::try_from(id)
        .ok()
        .and_then(|i| DyeColor::ALL.get(i).copied())
        .unwrap_or(DyeColor::ALL[0])
}

/// `componentLookup().findAll(DataComponents.DYE, color)`: the items whose default `dye` is `color`, in registry order.
fn find_all_dye_items(color: DyeColor) -> Vec<Item> {
    (0..mc_data::item::ITEM_DATA.len() as u32)
        .filter_map(Item::from_id)
        .filter(|item| ItemStack::new(*item).get(&DYE).is_some_and(|c| *c == color))
        .collect()
}

/// `findColorMixInRecipes(level, c1, c2)`.
fn find_color_mix_in_recipes(level: &ServerLevel, c1: DyeColor, c2: DyeColor) -> Option<DyeColor> {
    let dye1_items = find_all_dye_items(c1);
    if dye1_items.is_empty() {
        return None;
    }
    let dye2_items = find_all_dye_items(c2);
    if dye2_items.is_empty() {
        return None;
    }
    let manager = level.recipe_access();
    for dye1 in &dye1_items {
        for dye2 in &dye2_items {
            let input = CraftingInput::of(2, 1, &[ItemStack::new(*dye1), ItemStack::new(*dye2)]);
            if let Some(found) = manager.get_recipe_for(RecipeType::CRAFTING, &input, level, None) {
                let result = found.value.assemble_crafting(&input);
                if let Some(color) = result.get(&DYE) {
                    return Some(*color);
                }
            }
        }
    }
    None
}

/// `DyeColor.getMixedColor(level, dyeColor1, dyeColor2)`.
pub fn get_mixed_color(
    level: &mut ServerLevel,
    dye_color1: DyeColor,
    dye_color2: DyeColor,
) -> DyeColor {
    if let Some(mixed) = find_color_mix_in_recipes(level, dye_color1, dye_color2) {
        return mixed;
    }
    if level.random().next_boolean() {
        dye_color1
    } else {
        dye_color2
    }
}
