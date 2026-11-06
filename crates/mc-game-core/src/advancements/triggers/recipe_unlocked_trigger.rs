//! `RecipeUnlockedTrigger`: `recipe_unlocked`.

use super::prelude::*;
use mc_core::ident::Identifier;

/// `RecipeUnlockedTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct RecipeUnlockedTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub recipes: RecipeSet,
}

impl TriggerInstance for RecipeUnlockedTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<RecipeUnlockedTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(RecipeUnlockedTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    recipes: recipe_set_codec().field_of("recipes"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for RecipeUnlockedTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(RecipeUnlockedTriggerInstance);

crate::register_triggers!(RecipeUnlockedTriggerInstance {
    RECIPE_UNLOCKED = "recipe_unlocked",
});

impl RecipeUnlockedTriggerInstance {
    /// `matches(recipe)`: `recipes.stream().anyMatch(holder -> holder.is(recipe.id()))`.
    pub fn matches(
        &self,
        registries: &crate::registry::RegistryAccess,
        recipe: &Identifier,
    ) -> bool {
        self.recipes.contains(registries, recipe)
    }
}

impl Trigger<RecipeUnlockedTriggerInstance> {
    /// `trigger(player, recipe)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, recipe: &Identifier) {
        let registries = level.registries.clone();
        self.trigger_with(level, player, |t| t.matches(&registries, recipe));
    }
}

/// `HolderSet<Recipe<?>>` as the recipe triggers read it (`Recipe.LIST_CODEC` = `RegistryCodecs.homogeneousList(RECIPE)`):
/// a `#tag` or a list of recipe ids (a single id may be given bare).
#[derive(Clone, Debug, PartialEq)]
pub enum RecipeSet {
    Tag(Identifier),
    Elements(Vec<Identifier>),
}

impl RecipeSet {
    /// `stream().anyMatch(holder -> holder.is(id))`.
    pub fn contains(&self, registries: &crate::registry::RegistryAccess, id: &Identifier) -> bool {
        match self {
            RecipeSet::Elements(ids) => ids.contains(id),
            RecipeSet::Tag(tag) => registries.is_in_tag(crate::registry::keys::RECIPE, id, tag),
        }
    }
}

pub fn recipe_set_codec() -> Codec<RecipeSet> {
    use crate::world::level::storage::loot::holder::registry_ref_codec;
    use mc_dfu::extra::{IDENTIFIER, compact_list_codec_with};
    static C: std::sync::LazyLock<Codec<RecipeSet>> = std::sync::LazyLock::new(|| {
        let tag = STRING.comap_flat_map(
            |s: String| match s.strip_prefix('#') {
                None => DataResult::error(move || format!("Not a tag id: {s}")),
                Some(rest) => match Identifier::read(rest) {
                    Ok(id) => DataResult::success(id),
                    Err(e) => DataResult::error(move || e.clone()),
                },
            },
            |id: &Identifier| format!("#{id}"),
        );
        let element = registry_ref_codec(crate::registry::keys::RECIPE);
        let list = element.list_of();
        let direct = compact_list_codec_with(&element, &list);
        let _ = IDENTIFIER.clone();
        tag.either(&direct).xmap(
            |e| match e {
                mc_dfu::Either::Left(tag) => RecipeSet::Tag(tag),
                mc_dfu::Either::Right(ids) => RecipeSet::Elements(ids),
            },
            |s: &RecipeSet| match s {
                RecipeSet::Tag(t) => mc_dfu::Either::Left(t.clone()),
                RecipeSet::Elements(ids) => mc_dfu::Either::Right(ids.clone()),
            },
        )
    });
    C.clone()
}
