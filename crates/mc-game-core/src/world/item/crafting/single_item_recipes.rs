//! `AbstractCookingRecipe` (smelting, blasting, smoking, campfire cooking) and `StonecutterRecipe`.

use mc_data::{Item, RecipeBookCategory, RecipeSerializer, RecipeType};
use mc_dfu::{FLOAT, INT, MapCodec};
use mc_protocol::components::ItemStackTemplate;
use mc_protocol::packets::play::inventory::{FurnaceRecipeDisplay, RecipeDisplay, SlotDisplay, StonecutterRecipeDisplay};

use super::display::{item_display, stack_display};
use super::ingredient::{Ingredient, PlacementInfo};
use super::recipe::{CommonInfo, CookingBookCategory, CookingBookInfo};

/// Which of the four `AbstractCookingRecipe` subclasses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CookingKind {
    Smelting,
    Blasting,
    Smoking,
    CampfireCooking,
}

impl CookingKind {
    pub fn of_serializer(serializer: RecipeSerializer) -> Option<CookingKind> {
        [CookingKind::Smelting, CookingKind::Blasting, CookingKind::Smoking, CookingKind::CampfireCooking].into_iter().find(|k| k.serializer() == serializer)
    }

    pub fn serializer(self) -> RecipeSerializer {
        match self {
            CookingKind::Smelting => RecipeSerializer::SMELTING,
            CookingKind::Blasting => RecipeSerializer::BLASTING,
            CookingKind::Smoking => RecipeSerializer::SMOKING,
            CookingKind::CampfireCooking => RecipeSerializer::CAMPFIRE_COOKING,
        }
    }

    pub fn recipe_type(self) -> RecipeType {
        match self {
            CookingKind::Smelting => RecipeType::SMELTING,
            CookingKind::Blasting => RecipeType::BLASTING,
            CookingKind::Smoking => RecipeType::SMOKING,
            CookingKind::CampfireCooking => RecipeType::CAMPFIRE_COOKING,
        }
    }

    /// `furnaceIcon()`.
    fn furnace_icon(self) -> Item {
        match self {
            CookingKind::Smelting => Item::FURNACE,
            CookingKind::Blasting => Item::BLAST_FURNACE,
            CookingKind::Smoking => Item::SMOKER,
            CookingKind::CampfireCooking => Item::CAMPFIRE,
        }
    }
}

/// `AbstractCookingRecipe`.
#[derive(Clone, Debug)]
pub struct CookingRecipe {
    pub kind: CookingKind,
    pub common: CommonInfo,
    pub book: CookingBookInfo,
    pub input: Ingredient,
    pub result: ItemStackTemplate,
    pub experience: f32,
    pub cooking_time: i32,
}

impl CookingRecipe {
    /// `cookingMapCodec(factory)`.
    pub fn map_codec(kind: CookingKind) -> MapCodec<CookingRecipe> {
        let (common, book, input, result) = (CommonInfo::map_codec(), CookingBookInfo::map_codec(), Ingredient::codec().field_of("ingredient"), ItemStackTemplate::codec().field_of("result"));
        let (experience, cooking_time) = (FLOAT.optional_field_of_default("experience", 0.0f32), INT.field_of("cookingtime"));
        mc_dfu::record!(CookingRecipeFields {
            common: common,
            book: book,
            input: input,
            result: result,
            experience: experience,
            cooking_time: cooking_time,
        })
        .xmap(
            move |f: CookingRecipeFields| CookingRecipe {
                kind,
                common: f.common,
                book: f.book,
                input: f.input,
                result: f.result,
                experience: f.experience,
                cooking_time: f.cooking_time,
            },
            |r: &CookingRecipe| CookingRecipeFields {
                common: r.common,
                book: r.book.clone(),
                input: r.input.clone(),
                result: r.result.clone(),
                experience: r.experience,
                cooking_time: r.cooking_time,
            },
        )
    }

    /// `experience()`.
    pub fn experience(&self) -> f32 {
        self.experience
    }

    /// `cookingTime()`.
    pub fn cooking_time(&self) -> i32 {
        self.cooking_time
    }

    /// `category()`.
    pub fn category(&self) -> CookingBookCategory {
        self.book.category
    }

    /// `input()`.
    pub fn input(&self) -> &Ingredient {
        &self.input
    }

    /// `result()` (the template; `assemble` creates the stack).
    pub fn result(&self) -> &ItemStackTemplate {
        &self.result
    }

    pub(super) fn placement_info(&self) -> PlacementInfo {
        PlacementInfo::create_single(&self.input)
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        vec![RecipeDisplay::Furnace(FurnaceRecipeDisplay {
            ingredient: self.input.display(),
            fuel: SlotDisplay::AnyFuel,
            result: stack_display(&self.result),
            crafting_station: item_display(self.kind.furnace_icon()),
            duration: self.cooking_time,
            experience: self.experience,
        })]
    }

    /// `recipeBookCategory()`.
    pub(super) fn recipe_book_category(&self) -> RecipeBookCategory {
        match (self.kind, self.book.category) {
            (CookingKind::Smelting, CookingBookCategory::Blocks) => RecipeBookCategory::FURNACE_BLOCKS,
            (CookingKind::Smelting, CookingBookCategory::Food) => RecipeBookCategory::FURNACE_FOOD,
            (CookingKind::Smelting, CookingBookCategory::Misc) => RecipeBookCategory::FURNACE_MISC,
            (CookingKind::Blasting, CookingBookCategory::Blocks) => RecipeBookCategory::BLAST_FURNACE_BLOCKS,
            (CookingKind::Blasting, _) => RecipeBookCategory::BLAST_FURNACE_MISC,
            (CookingKind::Smoking, _) => RecipeBookCategory::SMOKER_FOOD,
            (CookingKind::CampfireCooking, _) => RecipeBookCategory::CAMPFIRE,
        }
    }
}

/// The record `cookingMapCodec` reads (everything but the kind).
#[derive(Clone)]
struct CookingRecipeFields {
    common: CommonInfo,
    book: CookingBookInfo,
    input: Ingredient,
    result: ItemStackTemplate,
    experience: f32,
    cooking_time: i32,
}

/// `StonecutterRecipe`.
#[derive(Clone, Debug)]
pub struct StonecutterRecipe {
    pub common: CommonInfo,
    pub input: Ingredient,
    pub result: ItemStackTemplate,
}

impl StonecutterRecipe {
    /// `simpleMapCodec(StonecutterRecipe::new)`.
    pub fn map_codec() -> MapCodec<StonecutterRecipe> {
        mc_dfu::record!(StonecutterRecipe {
            common: CommonInfo::map_codec(),
            input: Ingredient::codec().field_of("ingredient"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    pub fn input(&self) -> &Ingredient {
        &self.input
    }

    /// `resultDisplay()`.
    pub fn result_display(&self) -> SlotDisplay {
        stack_display(&self.result)
    }

    pub(super) fn placement_info(&self) -> PlacementInfo {
        PlacementInfo::create_single(&self.input)
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        vec![RecipeDisplay::Stonecutter(StonecutterRecipeDisplay {
            input: self.input.display(),
            result: self.result_display(),
            crafting_station: item_display(Item::STONECUTTER),
        })]
    }
}
