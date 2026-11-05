//! `Recipe`, `RecipeSerializer`s (the dispatch codec), book categories and the per-kind data.
//!
//! Vanilla's `Recipe<I>` interface hierarchy is a closed set of serializers (a data pack cannot add one), so a recipe is a
//! [`RecipeKind`] enum with one struct per vanilla class; [`Recipe`] adds the cached `placementInfo()`.

use mc_core::ident::Identifier;
use mc_data::{Item, RecipeBookCategory, RecipeSerializer, RecipeType};
use mc_dfu::{BOOL, Codec, DataResult, MapCodec, STRING, string_enum_codec};
use mc_protocol::components::holder::registry_by_name_codec;
use mc_protocol::packets::play::inventory::RecipeDisplay;

use super::brewing_recipe::BrewingRecipe;
use super::crafting_recipes::{ShapedRecipe, ShapelessRecipe};
use super::ingredient::{Ingredient, PlacementInfo};
use super::inputs::{BrewingInput, CraftingInput, SmithingRecipeInput};
use super::single_item_recipes::{CookingRecipe, StonecutterRecipe};
use super::smithing_recipes::{SmithingTransformRecipe, SmithingTrimRecipe};
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;

/// `CraftingBookCategory`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CraftingBookCategory {
    Building,
    Redstone,
    Equipment,
    Misc,
}

impl CraftingBookCategory {
    pub fn codec() -> Codec<CraftingBookCategory> {
        string_enum_codec(&[
            (CraftingBookCategory::Building, "building"),
            (CraftingBookCategory::Redstone, "redstone"),
            (CraftingBookCategory::Equipment, "equipment"),
            (CraftingBookCategory::Misc, "misc"),
        ])
    }
}

/// `CookingBookCategory`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CookingBookCategory {
    Food,
    Blocks,
    Misc,
}

impl CookingBookCategory {
    pub fn codec() -> Codec<CookingBookCategory> {
        string_enum_codec(&[(CookingBookCategory::Food, "food"), (CookingBookCategory::Blocks, "blocks"), (CookingBookCategory::Misc, "misc")])
    }
}

/// `Recipe.CommonInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CommonInfo {
    pub show_notification: bool,
}

impl CommonInfo {
    /// `CommonInfo.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<CommonInfo> {
        BOOL.optional_field_of_default("show_notification", true).xmap(|show_notification| CommonInfo { show_notification }, |c: &CommonInfo| c.show_notification)
    }
}

/// `CraftingRecipe.CraftingBookInfo`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CraftingBookInfo {
    pub category: CraftingBookCategory,
    pub group: String,
}

impl CraftingBookInfo {
    /// `CraftingBookInfo.MAP_CODEC` (`Recipe.BookInfo.mapCodec`).
    pub fn map_codec() -> MapCodec<CraftingBookInfo> {
        mc_dfu::record!(CraftingBookInfo {
            category: CraftingBookCategory::codec().optional_field_of_default("category", CraftingBookCategory::Misc),
            group: STRING.optional_field_of_default("group", String::new()),
        })
    }
}

/// `AbstractCookingRecipe.CookingBookInfo`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CookingBookInfo {
    pub category: CookingBookCategory,
    pub group: String,
}

impl CookingBookInfo {
    pub fn map_codec() -> MapCodec<CookingBookInfo> {
        mc_dfu::record!(CookingBookInfo {
            category: CookingBookCategory::codec().optional_field_of_default("category", CookingBookCategory::Misc),
            group: STRING.optional_field_of_default("group", String::new()),
        })
    }
}

/// One variant per vanilla `RecipeSerializer`.
#[derive(Clone, Debug)]
pub enum RecipeKind {
    Shaped(ShapedRecipe),
    Shapeless(ShapelessRecipe),
    Cooking(CookingRecipe),
    Stonecutting(StonecutterRecipe),
    SmithingTransform(SmithingTransformRecipe),
    SmithingTrim(SmithingTrimRecipe),
    Brewing(BrewingRecipe),
}

/// The three ingredients of a `SmithingRecipe` (`templateIngredient/baseIngredient/additionIngredient`).
pub struct SmithingIngredients<'a> {
    pub template: Option<&'a Ingredient>,
    pub base: &'a Ingredient,
    pub addition: Option<&'a Ingredient>,
}

/// `Recipe<?>`.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub kind: RecipeKind,
    placement_info: PlacementInfo,
}

impl Recipe {
    pub fn new(kind: RecipeKind) -> Recipe {
        let placement_info = match &kind {
            RecipeKind::Shaped(r) => r.create_placement_info(),
            RecipeKind::Shapeless(r) => r.create_placement_info(),
            RecipeKind::Cooking(r) => r.placement_info(),
            RecipeKind::Stonecutting(r) => r.placement_info(),
            RecipeKind::SmithingTransform(r) => r.create_placement_info(),
            RecipeKind::SmithingTrim(r) => r.create_placement_info(),
            RecipeKind::Brewing(_) => PlacementInfo::not_placeable(),
        };
        Recipe { kind, placement_info }
    }

    /// Binds every named ingredient to its tag in `registries` and recomputes `placementInfo()`.
    pub fn resolve(mut self, registries: &RegistryAccess) -> Recipe {
        self.for_each_ingredient_mut(&mut |i| i.resolve(registries));
        Recipe::new(self.kind)
    }

    fn for_each_ingredient_mut(&mut self, f: &mut dyn FnMut(&mut Ingredient)) {
        match &mut self.kind {
            RecipeKind::Shaped(r) => r.for_each_ingredient_mut(f),
            RecipeKind::Shapeless(r) => r.ingredients.iter_mut().for_each(f),
            RecipeKind::Cooking(r) => f(&mut r.input),
            RecipeKind::Stonecutting(r) => f(&mut r.input),
            RecipeKind::SmithingTransform(r) => {
                r.template.iter_mut().for_each(&mut *f);
                f(&mut r.base);
                r.addition.iter_mut().for_each(&mut *f);
            }
            RecipeKind::SmithingTrim(r) => {
                f(&mut r.template);
                f(&mut r.base);
                f(&mut r.addition);
            }
            RecipeKind::Brewing(r) => {
                f(&mut r.input.ingredient);
                f(&mut r.reagent.ingredient);
            }
        }
    }

    /// `getType()`.
    pub fn recipe_type(&self) -> RecipeType {
        match &self.kind {
            RecipeKind::Shaped(_) | RecipeKind::Shapeless(_) => RecipeType::CRAFTING,
            RecipeKind::Cooking(r) => r.kind.recipe_type(),
            RecipeKind::Stonecutting(_) => RecipeType::STONECUTTING,
            RecipeKind::SmithingTransform(_) | RecipeKind::SmithingTrim(_) => RecipeType::SMITHING,
            RecipeKind::Brewing(_) => RecipeType::BREWING,
        }
    }

    /// `getSerializer()`.
    pub fn serializer(&self) -> RecipeSerializer {
        match &self.kind {
            RecipeKind::Shaped(_) => RecipeSerializer::CRAFTING_SHAPED,
            RecipeKind::Shapeless(_) => RecipeSerializer::CRAFTING_SHAPELESS,
            RecipeKind::Cooking(r) => r.kind.serializer(),
            RecipeKind::Stonecutting(_) => RecipeSerializer::STONECUTTING,
            RecipeKind::SmithingTransform(_) => RecipeSerializer::SMITHING_TRANSFORM,
            RecipeKind::SmithingTrim(_) => RecipeSerializer::SMITHING_TRIM,
            RecipeKind::Brewing(_) => RecipeSerializer::BREWING,
        }
    }

    /// `isSpecial()` (custom recipes and brewing).
    pub fn is_special(&self) -> bool {
        matches!(self.kind, RecipeKind::Brewing(_))
    }

    /// `showNotification()`.
    pub fn show_notification(&self) -> bool {
        match &self.kind {
            RecipeKind::Shaped(r) => r.common.show_notification,
            RecipeKind::Shapeless(r) => r.common.show_notification,
            RecipeKind::Cooking(r) => r.common.show_notification,
            RecipeKind::Stonecutting(r) => r.common.show_notification,
            RecipeKind::SmithingTransform(r) => r.common.show_notification,
            RecipeKind::SmithingTrim(r) => r.common.show_notification,
            RecipeKind::Brewing(_) => false,
        }
    }

    /// `group()`.
    pub fn group(&self) -> &str {
        match &self.kind {
            RecipeKind::Shaped(r) => &r.book.group,
            RecipeKind::Shapeless(r) => &r.book.group,
            RecipeKind::Cooking(r) => &r.book.group,
            RecipeKind::Stonecutting(_) | RecipeKind::SmithingTransform(_) | RecipeKind::SmithingTrim(_) | RecipeKind::Brewing(_) => "",
        }
    }

    /// `placementInfo()`.
    pub fn placement_info(&self) -> &PlacementInfo {
        &self.placement_info
    }

    /// `display()`.
    pub fn display(&self) -> Vec<RecipeDisplay> {
        match &self.kind {
            RecipeKind::Shaped(r) => r.display(),
            RecipeKind::Shapeless(r) => r.display(),
            RecipeKind::Cooking(r) => r.display(),
            RecipeKind::Stonecutting(r) => r.display(),
            RecipeKind::SmithingTransform(r) => r.display(),
            RecipeKind::SmithingTrim(r) => r.display(),
            RecipeKind::Brewing(_) => Vec::new(),
        }
    }

    /// `recipeBookCategory()`.
    pub fn recipe_book_category(&self) -> RecipeBookCategory {
        match &self.kind {
            RecipeKind::Shaped(r) => crafting_category(r.book.category),
            RecipeKind::Shapeless(r) => crafting_category(r.book.category),
            RecipeKind::Cooking(r) => r.recipe_book_category(),
            RecipeKind::Stonecutting(_) => RecipeBookCategory::STONECUTTER,
            RecipeKind::SmithingTransform(_) | RecipeKind::SmithingTrim(_) => RecipeBookCategory::SMITHING,
            RecipeKind::Brewing(_) => RecipeBookCategory::CRAFTING_MISC,
        }
    }

    /// `CraftingRecipe.matches(input, level)` (false for recipes of another input kind).
    pub fn matches_crafting(&self, input: &CraftingInput, level: &ServerLevel) -> bool {
        let _ = level;
        match &self.kind {
            RecipeKind::Shaped(r) => r.matches(input),
            RecipeKind::Shapeless(r) => r.matches(input),
            _ => false,
        }
    }

    /// `CraftingRecipe.assemble(input)`.
    pub fn assemble_crafting(&self, input: &CraftingInput) -> ItemStack {
        match &self.kind {
            RecipeKind::Shaped(r) => r.result.create(),
            RecipeKind::Shapeless(r) => r.assemble(input),
            _ => ItemStack::empty(),
        }
    }

    /// `CraftingRecipe.getRemainingItems(input)` (one entry per slot of `input`).
    pub fn get_remaining_items(&self, input: &CraftingInput) -> Vec<ItemStack> {
        default_crafting_reminder(input)
    }

    /// `SmithingRecipe.matches(input, level)`.
    pub fn matches_smithing(&self, input: &SmithingRecipeInput) -> bool {
        match self.as_smithing() {
            Some(s) => {
                Ingredient::test_optional(s.template, &input.template) && s.base.test(&input.base) && Ingredient::test_optional(s.addition, &input.addition)
            }
            None => false,
        }
    }

    /// `SmithingRecipe.assemble(input)`.
    pub fn assemble_smithing(&self, input: &SmithingRecipeInput) -> ItemStack {
        match &self.kind {
            RecipeKind::SmithingTransform(r) => r.assemble(input),
            RecipeKind::SmithingTrim(r) => r.assemble(input),
            _ => ItemStack::empty(),
        }
    }

    /// `BrewingRecipe.matches(input)`.
    pub fn matches_brewing(&self, input: &BrewingInput) -> bool {
        matches!(&self.kind, RecipeKind::Brewing(r) if r.matches(input))
    }

    /// `BrewingRecipe.assemble(input)`.
    pub fn assemble_brewing(&self, _input: &BrewingInput) -> ItemStack {
        match &self.kind {
            RecipeKind::Brewing(r) => r.output.create(),
            _ => ItemStack::empty(),
        }
    }

    /// `instanceof AbstractCookingRecipe`.
    pub fn as_cooking(&self) -> Option<&CookingRecipe> {
        match &self.kind {
            RecipeKind::Cooking(r) => Some(r),
            _ => None,
        }
    }

    /// `instanceof StonecutterRecipe`.
    pub fn as_stonecutter(&self) -> Option<&StonecutterRecipe> {
        match &self.kind {
            RecipeKind::Stonecutting(r) => Some(r),
            _ => None,
        }
    }

    /// `instanceof SmithingRecipe`: `templateIngredient()/baseIngredient()/additionIngredient()`.
    pub fn as_smithing(&self) -> Option<SmithingIngredients<'_>> {
        match &self.kind {
            RecipeKind::SmithingTransform(r) => Some(SmithingIngredients { template: r.template.as_ref(), base: &r.base, addition: r.addition.as_ref() }),
            RecipeKind::SmithingTrim(r) => Some(SmithingIngredients { template: Some(&r.template), base: &r.base, addition: Some(&r.addition) }),
            _ => None,
        }
    }

    /// `instanceof BrewingRecipe`.
    pub fn as_brewing(&self) -> Option<&BrewingRecipe> {
        match &self.kind {
            RecipeKind::Brewing(r) => Some(r),
            _ => None,
        }
    }

    /// `instanceof CraftingRecipe`.
    pub fn is_crafting(&self) -> bool {
        self.recipe_type() == RecipeType::CRAFTING
    }

    /// `Recipe.DIRECT_CODEC`: the `type`-dispatched serializer codec.
    pub fn codec() -> Codec<Recipe> {
        registry_by_name_codec::<RecipeSerializer>().dispatch(
            "type",
            |recipe: &Recipe| recipe.serializer(),
            |serializer| {
                let codec = serializer_codec(*serializer);
                codec.xmap(Recipe::new, |recipe: &Recipe| recipe.kind.clone())
            },
        )
    }
}

fn crafting_category(category: CraftingBookCategory) -> RecipeBookCategory {
    match category {
        CraftingBookCategory::Building => RecipeBookCategory::CRAFTING_BUILDING_BLOCKS,
        CraftingBookCategory::Equipment => RecipeBookCategory::CRAFTING_EQUIPMENT,
        CraftingBookCategory::Redstone => RecipeBookCategory::CRAFTING_REDSTONE,
        CraftingBookCategory::Misc => RecipeBookCategory::CRAFTING_MISC,
    }
}

/// `CraftingRecipe.defaultCraftingReminder(input)`.
pub fn default_crafting_reminder(input: &CraftingInput) -> Vec<ItemStack> {
    input
        .items
        .iter()
        .map(|stack| match stack.item().data().crafting_remainder {
            Some(remainder) => ItemStack::new(remainder),
            None => ItemStack::empty(),
        })
        .collect()
}

/// The map codec of one serializer (`RecipeSerializer.codec()`).
fn serializer_codec(serializer: RecipeSerializer) -> MapCodec<RecipeKind> {
    if serializer == RecipeSerializer::CRAFTING_SHAPED {
        ShapedRecipe::map_codec().xmap(RecipeKind::Shaped, |k: &RecipeKind| match k {
            RecipeKind::Shaped(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else if serializer == RecipeSerializer::CRAFTING_SHAPELESS {
        ShapelessRecipe::map_codec().xmap(RecipeKind::Shapeless, |k: &RecipeKind| match k {
            RecipeKind::Shapeless(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else if let Some(kind) = super::single_item_recipes::CookingKind::of_serializer(serializer) {
        CookingRecipe::map_codec(kind).xmap(RecipeKind::Cooking, |k: &RecipeKind| match k {
            RecipeKind::Cooking(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else if serializer == RecipeSerializer::STONECUTTING {
        StonecutterRecipe::map_codec().xmap(RecipeKind::Stonecutting, |k: &RecipeKind| match k {
            RecipeKind::Stonecutting(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else if serializer == RecipeSerializer::SMITHING_TRANSFORM {
        SmithingTransformRecipe::map_codec().xmap(RecipeKind::SmithingTransform, |k: &RecipeKind| match k {
            RecipeKind::SmithingTransform(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else if serializer == RecipeSerializer::SMITHING_TRIM {
        SmithingTrimRecipe::map_codec().xmap(RecipeKind::SmithingTrim, |k: &RecipeKind| match k {
            RecipeKind::SmithingTrim(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else if serializer == RecipeSerializer::BREWING {
        BrewingRecipe::map_codec().xmap(RecipeKind::Brewing, |k: &RecipeKind| match k {
            RecipeKind::Brewing(r) => r.clone(),
            _ => unreachable!("serializer/kind mismatch"),
        })
    } else {
        MapCodec::from_fns(
            |_, _, _| {},
            move |_, _| {
                let name = mc_data::registry::by_name("minecraft:recipe_serializer").and_then(|r| r.name_of(serializer.id())).unwrap_or("?").to_owned();
                DataResult::error(move || format!("Recipe serializer {name} is not implemented"))
            },
            Vec::new,
        )
    }
}

/// `Item` helper used by displays: the crafting station icons.
pub(super) fn station(item: Item) -> mc_protocol::packets::play::inventory::SlotDisplay {
    super::display::item_display(item)
}

/// A recipe key (`ResourceKey<Recipe>`) is the path of its identifier in the `recipe` registry.
pub type RecipeId = Identifier;
