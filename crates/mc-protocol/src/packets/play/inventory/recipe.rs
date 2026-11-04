//! Recipe book packets and the recipe/slot display model.
use crate::{Decode, Encode, Packet};

/// `RecipeDisplayId`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct RecipeDisplayId {
    #[mc(var)]
    pub index: i32,
}

/// `RecipeBookType` (`readEnum`: VarInt ordinal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 4")]
pub enum RecipeBookType {
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
}

/// `RecipeBookSettings.TypeSettings.STREAM_CODEC`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct RecipeBookSettingsTypeSettings {
    pub open: bool,
    pub filtering: bool,
}

/// `RecipeBookSettings.STREAM_CODEC`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct RecipeBookSettings {
    pub crafting: RecipeBookSettingsTypeSettings,
    pub furnace: RecipeBookSettingsTypeSettings,
    pub blast_furnace: RecipeBookSettingsTypeSettings,
    pub smoker: RecipeBookSettingsTypeSettings,
}

/// `ClientboundRecipeBookSettingsPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "recipe_book_settings")]
pub struct ClientboundRecipeBookSettingsPacket {
    pub book_settings: RecipeBookSettings,
}

/// `ClientboundRecipeBookRemovePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "recipe_book_remove")]
pub struct ClientboundRecipeBookRemovePacket {
    pub recipes: Vec<RecipeDisplayId>,
}

/// `ServerboundPlaceRecipePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "place_recipe")]
pub struct ServerboundPlaceRecipePacket {
    #[mc(var)]
    pub container_id: i32,
    pub recipe: RecipeDisplayId,
    pub use_max_items: bool,
}

/// `ServerboundRecipeBookChangeSettingsPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "recipe_book_change_settings")]
pub struct ServerboundRecipeBookChangeSettingsPacket {
    pub book_type: RecipeBookType,
    pub is_open: bool,
    pub is_filtering: bool,
}

/// `ServerboundRecipeBookSeenRecipePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "recipe_book_seen_recipe")]
pub struct ServerboundRecipeBookSeenRecipePacket {
    pub recipe: RecipeDisplayId,
}
