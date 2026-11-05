//! `RecipeBookSettings` + `ServerRecipeBook` (the per-player set of known recipes and the ones still highlighted as new),
//! with the `recipeBook` NBT of the player file (`ServerRecipeBook.Packed.CODEC`).

use indexmap::IndexSet;
use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::packets::play::inventory::{
    ClientboundRecipeBookAddEntry, ClientboundRecipeBookAddPacket, ClientboundRecipeBookRemovePacket, ClientboundRecipeBookSettingsPacket, RecipeBookSettings as WireSettings,
    RecipeBookSettingsTypeSettings, RecipeBookType, RecipeDisplayId,
};

use super::recipe_manager::{RecipeHolder, RecipeManager};

/// The NBT field names of `RecipeBookSettings.TypeSettings` per book type: (open, filtering).
fn setting_keys(ty: RecipeBookType) -> (&'static str, &'static str) {
    match ty {
        RecipeBookType::Crafting => ("isGuiOpen", "isFilteringCraftable"),
        RecipeBookType::Furnace => ("isFurnaceGuiOpen", "isFurnaceFilteringCraftable"),
        RecipeBookType::BlastFurnace => ("isBlastingFurnaceGuiOpen", "isBlastingFurnaceFilteringCraftable"),
        RecipeBookType::Smoker => ("isSmokerGuiOpen", "isSmokerFilteringCraftable"),
    }
}

const BOOK_TYPES: [RecipeBookType; 4] = [RecipeBookType::Crafting, RecipeBookType::Furnace, RecipeBookType::BlastFurnace, RecipeBookType::Smoker];

/// `RecipeBookSettings` (the wire type doubles as the value: four `TypeSettings`).
pub type RecipeBookSettings = WireSettings;

fn settings_of(settings: &RecipeBookSettings, ty: RecipeBookType) -> RecipeBookSettingsTypeSettings {
    match ty {
        RecipeBookType::Crafting => settings.crafting,
        RecipeBookType::Furnace => settings.furnace,
        RecipeBookType::BlastFurnace => settings.blast_furnace,
        RecipeBookType::Smoker => settings.smoker,
    }
}

fn settings_mut(settings: &mut RecipeBookSettings, ty: RecipeBookType) -> &mut RecipeBookSettingsTypeSettings {
    match ty {
        RecipeBookType::Crafting => &mut settings.crafting,
        RecipeBookType::Furnace => &mut settings.furnace,
        RecipeBookType::BlastFurnace => &mut settings.blast_furnace,
        RecipeBookType::Smoker => &mut settings.smoker,
    }
}

/// `ServerRecipeBook`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerRecipeBook {
    book_settings: RecipeBookSettings,
    known: IndexSet<Identifier>,
    highlight: IndexSet<Identifier>,
}

impl ServerRecipeBook {
    pub fn new() -> ServerRecipeBook {
        ServerRecipeBook::default()
    }

    pub fn is_open(&self, ty: RecipeBookType) -> bool {
        settings_of(&self.book_settings, ty).open
    }

    pub fn set_open(&mut self, ty: RecipeBookType, open: bool) {
        settings_mut(&mut self.book_settings, ty).open = open;
    }

    pub fn is_filtering(&self, ty: RecipeBookType) -> bool {
        settings_of(&self.book_settings, ty).filtering
    }

    pub fn set_filtering(&mut self, ty: RecipeBookType, filtering: bool) {
        settings_mut(&mut self.book_settings, ty).filtering = filtering;
    }

    /// `setBookSetting(type, open, filtering)`.
    pub fn set_book_setting(&mut self, ty: RecipeBookType, open: bool, filtering: bool) {
        self.set_open(ty, open);
        self.set_filtering(ty, filtering);
    }

    pub fn book_settings(&self) -> &RecipeBookSettings {
        &self.book_settings
    }

    pub fn add(&mut self, id: Identifier) {
        self.known.insert(id);
    }

    pub fn contains(&self, id: &Identifier) -> bool {
        self.known.contains(id)
    }

    /// `remove(id)`: known and highlighted.
    pub fn remove(&mut self, id: &Identifier) {
        self.known.shift_remove(id);
        self.highlight.shift_remove(id);
    }

    pub fn remove_highlight(&mut self, id: &Identifier) {
        self.highlight.shift_remove(id);
    }

    pub fn is_highlighted(&self, id: &Identifier) -> bool {
        self.highlight.contains(id)
    }

    pub fn known(&self) -> impl Iterator<Item = &Identifier> {
        self.known.iter()
    }

    pub fn highlighted(&self) -> impl Iterator<Item = &Identifier> {
        self.highlight.iter()
    }

    /// The learning step of `addRecipes` for one recipe: `true` (and the display entries appended) when it was newly added; the
    /// caller then fires `RECIPE_UNLOCKED` and, after the loop, sends the packet ([`ServerRecipeBook::add_packet`]).
    pub fn try_add_recipe(&mut self, recipe: &RecipeHolder, manager: &RecipeManager, entries: &mut Vec<ClientboundRecipeBookAddEntry>) -> bool {
        if self.known.contains(&recipe.id) || recipe.value.is_special() {
            return false;
        }
        self.known.insert(recipe.id.clone());
        self.highlight.insert(recipe.id.clone());
        let notification = recipe.value.show_notification();
        manager.list_displays_for_recipe(&recipe.id, |display| entries.push(ClientboundRecipeBookAddEntry::new(display.clone(), notification, true)));
        true
    }

    /// The packet `addRecipes` sends when at least one display was added.
    pub fn add_packet(entries: Vec<ClientboundRecipeBookAddEntry>) -> Option<ClientboundRecipeBookAddPacket> {
        (!entries.is_empty()).then_some(ClientboundRecipeBookAddPacket { entries, replace: false })
    }

    /// `removeRecipes(recipes, player)`: the removal packet (none when nothing was removed) and the removed display count.
    pub fn remove_recipes(&mut self, recipes: &[RecipeHolder], manager: &RecipeManager) -> (Option<ClientboundRecipeBookRemovePacket>, i32) {
        let mut to_remove: Vec<RecipeDisplayId> = Vec::new();
        for recipe in recipes {
            if self.known.contains(&recipe.id) {
                self.remove(&recipe.id);
                manager.list_displays_for_recipe(&recipe.id, |display| to_remove.push(display.id));
            }
        }
        let count = to_remove.len() as i32;
        ((!to_remove.is_empty()).then_some(ClientboundRecipeBookRemovePacket { recipes: to_remove }), count)
    }

    /// `sendInitialRecipeBook(player)`: the settings packet then the full add packet (`replace = true`).
    pub fn initial_packets(&self, manager: &RecipeManager) -> (ClientboundRecipeBookSettingsPacket, ClientboundRecipeBookAddPacket) {
        let mut entries = Vec::with_capacity(self.known.len());
        for id in &self.known {
            let highlighted = self.highlight.contains(id);
            manager.list_displays_for_recipe(id, |display| entries.push(ClientboundRecipeBookAddEntry::new(display.clone(), false, highlighted)));
        }
        (ClientboundRecipeBookSettingsPacket { book_settings: self.book_settings }, ClientboundRecipeBookAddPacket { entries, replace: true })
    }

    /// `pack()` as the `recipeBook` compound of the player file (`Packed.CODEC`).
    pub fn pack(&self) -> Tag {
        let mut tag = CompoundTag::new();
        for ty in BOOK_TYPES {
            let (open_key, filtering_key) = setting_keys(ty);
            let s = settings_of(&self.book_settings, ty);
            if s.open {
                tag.put_boolean(open_key, true);
            }
            if s.filtering {
                tag.put_boolean(filtering_key, true);
            }
        }
        let list = |ids: &IndexSet<Identifier>| {
            let mut l = ListTag::new();
            ids.iter().for_each(|id| l.push(Tag::String(id.to_string())));
            Tag::List(l)
        };
        tag.put("recipes", list(&self.known));
        tag.put("toBeDisplayed", list(&self.highlight));
        Tag::Compound(tag)
    }

    /// `loadUntrusted(packed, validator)` of a stored `recipeBook` tag; a malformed tag (a missing/invalid list) loads nothing,
    /// unknown recipes are dropped with vanilla's error log.
    pub fn load_untrusted(&mut self, tag: &Tag, is_valid: impl Fn(&Identifier) -> bool) {
        let Some(compound) = tag.as_compound() else { return };
        let read_ids = |key: &str| -> Option<Vec<Identifier>> {
            let list = compound.get(key)?.as_list()?;
            (0..list.len()).map(|i| list.get(i).and_then(Tag::as_string).and_then(Identifier::try_parse)).collect()
        };
        let (Some(known), Some(highlight)) = (read_ids("recipes"), read_ids("toBeDisplayed")) else { return };
        for ty in BOOK_TYPES {
            let (open_key, filtering_key) = setting_keys(ty);
            let flag = |key: &str| compound.get(key).and_then(Tag::as_boolean).unwrap_or(false);
            *settings_mut(&mut self.book_settings, ty) = RecipeBookSettingsTypeSettings { open: flag(open_key), filtering: flag(filtering_key) };
        }
        for (ids, target) in [(known, &mut self.known), (highlight, &mut self.highlight)] {
            for id in ids {
                if is_valid(&id) {
                    target.insert(id);
                } else {
                    log::error!("Tried to load unrecognized recipe: {id} removed now.");
                }
            }
        }
    }

    /// `copyOverData(book)` (`apply(book.pack())`).
    pub fn copy_over_data(&mut self, other: &ServerRecipeBook) {
        *self = other.clone();
    }
}

impl ServerRecipeBook {
    /// Drops the recipes the manager does not know (the `loadUntrusted` validator, applied once a manager is at hand).
    pub fn retain_known(&mut self, manager: &RecipeManager) {
        for set in [&mut self.known, &mut self.highlight] {
            set.retain(|id| {
                let known = manager.by_key(id).is_some();
                if !known {
                    log::error!("Tried to load unrecognized recipe: {id} removed now.");
                }
                known
            });
        }
    }
}
