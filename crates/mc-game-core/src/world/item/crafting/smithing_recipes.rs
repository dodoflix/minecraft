//! `SmithingTransformRecipe` and `SmithingTrimRecipe`.

use mc_data::Item;
use mc_dfu::MapCodec;
use mc_protocol::codec::Holder as WireHolder;
use mc_protocol::components::values::{ArmorTrim, PROVIDES_TRIM_MATERIAL, TRIM, TrimPattern};
use mc_protocol::components::{Holder, ItemStackTemplate};
use mc_protocol::packets::play::inventory::{RecipeDisplay, SlotDisplay, SmithingRecipeDisplay};

use super::display::{item_display, stack_display};
use super::ingredient::{Ingredient, PlacementInfo};
use super::inputs::SmithingRecipeInput;
use super::recipe::CommonInfo;
use super::util::create_with_original_components;
use crate::world::item::ItemStack;

/// `SmithingTransformRecipe`.
#[derive(Clone, Debug)]
pub struct SmithingTransformRecipe {
    pub common: CommonInfo,
    pub template: Option<Ingredient>,
    pub base: Ingredient,
    pub addition: Option<Ingredient>,
    pub result: ItemStackTemplate,
}

impl SmithingTransformRecipe {
    pub fn map_codec() -> MapCodec<SmithingTransformRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(SmithingTransformRecipe {
            common: CommonInfo::map_codec(),
            template: ingredient.optional_field_of("template"),
            base: ingredient.field_of("base"),
            addition: ingredient.optional_field_of("addition"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    /// `assemble(input)`: the result with the base's components carried over.
    pub fn assemble(&self, input: &SmithingRecipeInput) -> ItemStack {
        create_with_original_components(&self.result, &input.base, 0)
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        PlacementInfo::create_from_optionals(&[
            self.template.as_ref(),
            Some(&self.base),
            self.addition.as_ref(),
        ])
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        vec![RecipeDisplay::Smithing(SmithingRecipeDisplay {
            template: Ingredient::optional_display(self.template.as_ref()),
            base: self.base.display(),
            addition: Ingredient::optional_display(self.addition.as_ref()),
            result: stack_display(&self.result),
            crafting_station: item_display(Item::SMITHING_TABLE),
        })]
    }
}

/// `SmithingTrimRecipe`.
#[derive(Clone, Debug)]
pub struct SmithingTrimRecipe {
    pub common: CommonInfo,
    pub template: Ingredient,
    pub base: Ingredient,
    pub addition: Ingredient,
    pub pattern: Holder<TrimPattern>,
}

impl SmithingTrimRecipe {
    pub fn map_codec() -> MapCodec<SmithingTrimRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(SmithingTrimRecipe {
            common: CommonInfo::map_codec(),
            template: ingredient.field_of("template"),
            base: ingredient.field_of("base"),
            addition: ingredient.field_of("addition"),
            pattern: TrimPattern::codec().field_of("pattern"),
        })
    }

    /// `assemble(input)`.
    pub fn assemble(&self, input: &SmithingRecipeInput) -> ItemStack {
        apply_trim(&input.base, &input.addition, &self.pattern)
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        PlacementInfo::create(&[
            self.template.clone(),
            self.base.clone(),
            self.addition.clone(),
        ])
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        let (base, material, template) = (
            self.base.display(),
            self.addition.display(),
            self.template.display(),
        );
        let pattern = match &self.pattern {
            Holder::Reference(name) => WireHolder::Reference(
                mc_protocol::registry_context::registry_id("minecraft:trim_pattern", name)
                    .unwrap_or(0),
            ),
            Holder::Direct(p) => {
                WireHolder::Direct(mc_protocol::packets::play::inventory::TrimPattern {
                    asset_id: p.asset_id.clone(),
                    description: p.description.clone(),
                    decal: p.decal,
                })
            }
        };
        vec![RecipeDisplay::Smithing(SmithingRecipeDisplay {
            template,
            base: base.clone(),
            addition: material.clone(),
            result: SlotDisplay::SmithingTrim {
                base: Box::new(base),
                material: Box::new(material),
                pattern,
            },
            crafting_station: item_display(Item::SMITHING_TABLE),
        })]
    }
}

/// `SmithingTrimRecipe.applyTrim(baseItem, materialItem, pattern)`.
pub fn apply_trim(
    base: &ItemStack,
    material_item: &ItemStack,
    pattern: &Holder<TrimPattern>,
) -> ItemStack {
    let Some(material) = material_item.get(&PROVIDES_TRIM_MATERIAL) else {
        return ItemStack::empty();
    };
    let new_trim = ArmorTrim {
        material: material.clone(),
        pattern: pattern.clone(),
    };
    if base.get(&TRIM) == Some(&new_trim) {
        return ItemStack::empty();
    }
    let mut trimmed = base.copy_with_count(1);
    trimmed.set(TRIM, new_trim);
    trimmed
}
