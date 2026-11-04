//! Recipe/slot display model and the packets carrying it (recipe book add, ghost recipe, update recipes).
use mc_core::ident::Identifier;
use mc_text::Component;

use super::recipe::RecipeDisplayId;
use super::untrusted_text;
use crate::codec::{read_count, read_varint, DecodeError, DecodeResult, Holder, HolderSet};
use crate::components::ItemStackTemplate;
use crate::{Decode, Encode, Packet};

mod optional_var_int {
    use crate::codec::OptionalVarInt;
    use crate::{Decode, Encode};

    pub fn encode(v: &Option<i32>, out: &mut Vec<u8>) {
        OptionalVarInt(*v).encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> crate::codec::DecodeResult<Option<i32>> {
        OptionalVarInt::decode(input).map(|v| v.0)
    }
}

/// `ByteBufCodecs.holderSet(Registries.ITEM)`: the `HolderSet` wire form; direct ids must be existing items.
/// (A named set can only be resolved against the synced tags, so it is kept as the tag name.)
mod item_holder_set {
    use crate::codec::{DecodeError, DecodeResult, HolderSet};
    use crate::{Decode, Encode};

    pub fn encode(set: &HolderSet, out: &mut Vec<u8>) {
        set.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<HolderSet> {
        // Vanilla resolves each id while reading; here the ids are checked after the whole set was read.
        let set = HolderSet::decode(input)?;
        if let HolderSet::Direct(ids) = &set {
            if let Some(bad) = ids.iter().find(|id| u32::try_from(**id).ok().and_then(mc_data::Item::from_id).is_none()) {
                return Err(DecodeError::Message(format!("No value with id {bad}")));
            }
        }
        Ok(set)
    }
}

/// `Ingredient.CONTENTS_STREAM_CODEC`: an item `HolderSet`; a direct set must be non-empty and free of air.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct Ingredient {
    pub values: HolderSet,
}

impl Decode for Ingredient {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let values = item_holder_set::decode(input)?;
        if let HolderSet::Direct(ids) = &values {
            if ids.is_empty() {
                return Err(DecodeError::msg("Ingredients can't be empty"));
            }
            if ids.contains(&(mc_data::Item::AIR.id() as i32)) {
                return Err(DecodeError::msg("Ingredient can't contain air"));
            }
        }
        Ok(Self { values })
    }
}

/// `TrimPattern.DIRECT_STREAM_CODEC` (the inline form of a `Holder<TrimPattern>`).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TrimPattern {
    pub asset_id: Identifier,
    #[mc(with = untrusted_text::component)]
    pub description: Component,
    pub decal: bool,
}

/// `SlotDisplay.STREAM_CODEC`: VarInt registry id of the display type (`slot_display` registry order), then its fields.
/// Variants are named after the vanilla classes without the `SlotDisplay` suffix; `Dyed` is `DyedSlotDemo`.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[mc(error = "No value with id {}")]
pub enum SlotDisplay {
    /// `SlotDisplay.Empty`
    Empty,
    /// `SlotDisplay.AnyFuel`
    AnyFuel,
    /// `SlotDisplay.WithAnyPotion`
    WithAnyPotion { display: Box<SlotDisplay> },
    /// `SlotDisplay.OnlyWithComponent`
    OnlyWithComponent { source: Box<SlotDisplay>, component: mc_data::DataComponentType },
    /// `SlotDisplay.ItemSlotDisplay`
    Item { item: mc_data::Item },
    /// `SlotDisplay.ItemStackSlotDisplay`
    ItemStack { stack: ItemStackTemplate },
    /// `SlotDisplay.TagSlotDisplay`
    Tag {
        #[mc(with = item_holder_set)]
        tag: HolderSet,
    },
    /// `SlotDisplay.DyedSlotDemo`
    Dyed { dye: Box<SlotDisplay>, target: Box<SlotDisplay> },
    /// `SlotDisplay.SmithingTrimDemoSlotDisplay`; `pattern` is a `Holder<TrimPattern>` (data-driven registry: raw id).
    SmithingTrim { base: Box<SlotDisplay>, material: Box<SlotDisplay>, pattern: Holder<TrimPattern> },
    /// `SlotDisplay.WithRemainder`
    WithRemainder { input: Box<SlotDisplay>, remainder: Box<SlotDisplay> },
    /// `SlotDisplay.Composite`
    Composite { contents: Vec<SlotDisplay> },
}

/// `ShapelessCraftingRecipeDisplay`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ShapelessCraftingRecipeDisplay {
    pub ingredients: Vec<SlotDisplay>,
    pub result: SlotDisplay,
    pub crafting_station: SlotDisplay,
}

/// `ShapedCraftingRecipeDisplay`; decoding enforces `ingredients.len() == width * height` (Java `int` multiplication).
#[derive(Debug, Clone, PartialEq, Encode)]
pub struct ShapedCraftingRecipeDisplay {
    #[mc(var)]
    pub width: i32,
    #[mc(var)]
    pub height: i32,
    pub ingredients: Vec<SlotDisplay>,
    pub result: SlotDisplay,
    pub crafting_station: SlotDisplay,
}

impl Decode for ShapedCraftingRecipeDisplay {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let width = read_varint(input)?;
        let height = read_varint(input)?;
        let ingredients = Vec::<SlotDisplay>::decode(input)?;
        let result = SlotDisplay::decode(input)?;
        let crafting_station = SlotDisplay::decode(input)?;
        if i32::try_from(ingredients.len()) != Ok(width.wrapping_mul(height)) {
            return Err(DecodeError::msg("Invalid shaped recipe display contents"));
        }
        Ok(Self { width, height, ingredients, result, crafting_station })
    }
}

/// `FurnaceRecipeDisplay`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct FurnaceRecipeDisplay {
    pub ingredient: SlotDisplay,
    pub fuel: SlotDisplay,
    pub result: SlotDisplay,
    pub crafting_station: SlotDisplay,
    #[mc(var)]
    pub duration: i32,
    pub experience: f32,
}

/// `StonecutterRecipeDisplay`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct StonecutterRecipeDisplay {
    pub input: SlotDisplay,
    pub result: SlotDisplay,
    pub crafting_station: SlotDisplay,
}

/// `SmithingRecipeDisplay`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SmithingRecipeDisplay {
    pub template: SlotDisplay,
    pub base: SlotDisplay,
    pub addition: SlotDisplay,
    pub result: SlotDisplay,
    pub crafting_station: SlotDisplay,
}

/// `RecipeDisplay.STREAM_CODEC`: VarInt registry id of the display type (`recipe_display` registry order), then its fields.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[mc(error = "No value with id {}")]
pub enum RecipeDisplay {
    CraftingShapeless(ShapelessCraftingRecipeDisplay),
    CraftingShaped(ShapedCraftingRecipeDisplay),
    Furnace(FurnaceRecipeDisplay),
    Stonecutter(StonecutterRecipeDisplay),
    Smithing(SmithingRecipeDisplay),
}

/// `RecipeDisplayEntry.STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RecipeDisplayEntry {
    pub id: RecipeDisplayId,
    pub display: RecipeDisplay,
    #[mc(with = optional_var_int)]
    pub group: Option<i32>,
    pub category: mc_data::RecipeBookCategory,
    pub crafting_requirements: Option<Vec<Ingredient>>,
}

/// `ClientboundRecipeBookAddPacket.Entry`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ClientboundRecipeBookAddEntry {
    pub contents: RecipeDisplayEntry,
    pub flags: i8,
}

impl ClientboundRecipeBookAddEntry {
    pub const FLAG_NOTIFICATION: i8 = 1;
    pub const FLAG_HIGHLIGHT: i8 = 2;

    pub fn new(contents: RecipeDisplayEntry, notification: bool, highlight: bool) -> Self {
        Self { contents, flags: i8::from(notification) | i8::from(highlight) << 1 }
    }
    pub fn notification(&self) -> bool {
        self.flags & Self::FLAG_NOTIFICATION != 0
    }
    pub fn highlight(&self) -> bool {
        self.flags & Self::FLAG_HIGHLIGHT != 0
    }
}

/// `ClientboundRecipeBookAddPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "recipe_book_add")]
pub struct ClientboundRecipeBookAddPacket {
    pub entries: Vec<ClientboundRecipeBookAddEntry>,
    pub replace: bool,
}

/// `ClientboundPlaceGhostRecipePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "place_ghost_recipe")]
pub struct ClientboundPlaceGhostRecipePacket {
    #[mc(var)]
    pub container_id: i32,
    pub recipe_display: RecipeDisplay,
}

/// `RecipePropertySet.STREAM_CODEC`: a set of items on the wire (a list; `Set.copyOf` drops repeated items on decode).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecipePropertySet {
    pub items: Vec<mc_data::Item>,
}

impl Encode for RecipePropertySet {
    fn encode(&self, out: &mut Vec<u8>) {
        self.items.encode(out)
    }
}
impl Decode for RecipePropertySet {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let mut items = Vec::<mc_data::Item>::decode(input)?;
        let mut seen = std::collections::HashSet::new();
        items.retain(|i| seen.insert(*i));
        Ok(Self { items })
    }
}

/// `SelectableRecipe` as sent by `SelectableRecipe.noRecipeCodec()`: only the display (the recipe holder stays server side).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SelectableRecipe {
    pub option_display: SlotDisplay,
}

/// `SelectableRecipe.SingleInputEntry` (no-recipe codec)
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SelectableRecipeSingleInputEntry {
    pub input: Ingredient,
    pub recipe: SelectableRecipe,
}

/// `SelectableRecipe.SingleInputSet` (no-recipe codec)
#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct SelectableRecipeSingleInputSet {
    pub entries: Vec<SelectableRecipeSingleInputEntry>,
}

/// `ClientboundUpdateRecipesPacket`: `item_sets` is a `Map<ResourceKey<RecipePropertySet>, RecipePropertySet>` in wire
/// order (a repeated key replaces the earlier value on decode).
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "update_recipes")]
pub struct ClientboundUpdateRecipesPacket {
    #[mc(with = item_sets)]
    pub item_sets: Vec<(Identifier, RecipePropertySet)>,
    pub stonecutter_recipes: SelectableRecipeSingleInputSet,
}

mod item_sets {
    use super::*;

    pub fn encode(sets: &[(Identifier, RecipePropertySet)], out: &mut Vec<u8>) {
        sets.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<(Identifier, RecipePropertySet)>> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut sets: Vec<(Identifier, RecipePropertySet)> = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            let (key, set) = (Identifier::decode(input)?, RecipePropertySet::decode(input)?);
            match sets.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1 = set,
                None => sets.push((key, set)),
            }
        }
        Ok(sets)
    }
}
