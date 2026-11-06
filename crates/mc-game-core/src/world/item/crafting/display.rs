//! Recipe/slot display helpers: the wire types come from `mc_protocol` (`SlotDisplay`, `RecipeDisplay`, ...); this adds the
//! `isEnabled(enabledFeatures)` checks of `RecipeDisplay`/`SlotDisplay`.

use mc_data::Item;
use mc_protocol::components::ItemStackTemplate;
use mc_protocol::packets::play::inventory::{RecipeDisplay, SlotDisplay};
use mc_world::level_storage::FeatureFlagSet;

use crate::world::item::item_is_enabled;

/// `SlotDisplay.ItemSlotDisplay(item)`.
pub fn item_display(item: Item) -> SlotDisplay {
    SlotDisplay::Item { item }
}

/// `SlotDisplay.ItemStackSlotDisplay(template)`.
pub fn stack_display(template: &ItemStackTemplate) -> SlotDisplay {
    SlotDisplay::ItemStack {
        stack: template.clone(),
    }
}

/// `SlotDisplay.isEnabled(enabledFeatures)`.
pub fn slot_display_enabled(display: &SlotDisplay, flags: FeatureFlagSet) -> bool {
    match display {
        SlotDisplay::Composite { contents } => {
            contents.iter().all(|c| slot_display_enabled(c, flags))
        }
        SlotDisplay::Item { item } => item_is_enabled(*item, flags),
        SlotDisplay::ItemStack { stack } => item_is_enabled(stack.item, flags),
        SlotDisplay::WithRemainder { input, remainder } => {
            slot_display_enabled(input, flags) && slot_display_enabled(remainder, flags)
        }
        _ => true,
    }
}

/// `RecipeDisplay.isEnabled(enabledFeatures)` including each display type's own override.
pub fn recipe_display_enabled(display: &RecipeDisplay, flags: FeatureFlagSet) -> bool {
    let (result, station) = match display {
        RecipeDisplay::CraftingShapeless(d) => {
            if !d.ingredients.iter().all(|i| slot_display_enabled(i, flags)) {
                return false;
            }
            (&d.result, &d.crafting_station)
        }
        RecipeDisplay::CraftingShaped(d) => {
            if !d.ingredients.iter().all(|i| slot_display_enabled(i, flags)) {
                return false;
            }
            (&d.result, &d.crafting_station)
        }
        RecipeDisplay::Furnace(d) => {
            if !(slot_display_enabled(&d.ingredient, flags) && slot_display_enabled(&d.fuel, flags))
            {
                return false;
            }
            (&d.result, &d.crafting_station)
        }
        RecipeDisplay::Stonecutter(d) => (&d.result, &d.crafting_station),
        RecipeDisplay::Smithing(d) => (&d.result, &d.crafting_station),
    };
    slot_display_enabled(result, flags) && slot_display_enabled(station, flags)
}
