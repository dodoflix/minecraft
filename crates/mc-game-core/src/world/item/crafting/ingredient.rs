//! `Ingredient` (an item `HolderSet`) and `PlacementInfo`.
//!
//! Recipes are decoded while the item tags may not be bound yet; a named set therefore keeps its tag id and is resolved to
//! the member items when the [`RecipeManager`](super::RecipeManager) is built ([`Ingredient::resolve`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_data::Item;
use mc_dfu::{Codec, DataResult, Either};
use mc_protocol::codec::HolderSet as WireHolderSet;
use mc_protocol::components::common::hashed_tag_codec;
use mc_protocol::components::holder::registry_by_name_codec;
use mc_protocol::packets::play::inventory::SlotDisplay;

use crate::registry::{RegistryAccess, keys};
use crate::world::item::ItemStack;

/// `HolderSet<Item>`: a tag (without the `#`) or an explicit list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemHolderSet {
    Named(Identifier),
    Direct(Vec<Item>),
}

/// `Ingredient`.
#[derive(Clone, Debug)]
pub struct Ingredient {
    values: ItemHolderSet,
    /// The member items (for a named set: as of the last [`Ingredient::resolve`]), in tag order.
    resolved: Arc<[Item]>,
}

impl PartialEq for Ingredient {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

thread_local! {
    /// Tags of the item registry known to exist while a recipe is being decoded (`HolderSetCodec.lookupTag`); filled by the
    /// registry decoder from the loading context.
    static TAG_EXISTS: RefCell<Option<HashMap<Identifier, bool>>> = const { RefCell::new(None) };
}

/// Runs `f` with the answers of `tag_exists` for the item tags `tags` installed for [`Ingredient::codec`].
pub fn with_item_tag_lookup<R>(tags: HashMap<Identifier, bool>, f: impl FnOnce() -> R) -> R {
    let previous = TAG_EXISTS.with(|t| t.replace(Some(tags)));
    let result = f();
    TAG_EXISTS.with(|t| *t.borrow_mut() = previous);
    result
}

fn item_tag_exists(tag: &Identifier) -> bool {
    TAG_EXISTS.with(|t| match t.borrow().as_ref() {
        Some(known) => known.get(tag).copied().unwrap_or(false),
        None => mc_protocol::registry_context::get().tag_exists("minecraft:item", tag),
    })
}

impl Ingredient {
    fn from_set(values: ItemHolderSet) -> Ingredient {
        let resolved: Arc<[Item]> = match &values {
            ItemHolderSet::Direct(items) => items.clone().into(),
            ItemHolderSet::Named(_) => Arc::new([]),
        };
        Ingredient { values, resolved }
    }

    /// `Ingredient.of(item)`.
    pub fn of(item: Item) -> Ingredient {
        Self::from_set(ItemHolderSet::Direct(vec![item]))
    }

    /// `Ingredient.of(items...)`.
    pub fn of_items(items: impl IntoIterator<Item = Item>) -> Ingredient {
        Self::from_set(ItemHolderSet::Direct(items.into_iter().collect()))
    }

    /// `Ingredient.of(tag)` (resolved by [`Ingredient::resolve`]).
    pub fn of_tag(tag: Identifier) -> Ingredient {
        Self::from_set(ItemHolderSet::Named(tag))
    }

    pub fn values(&self) -> &ItemHolderSet {
        &self.values
    }

    /// Binds a named set to the members of its tag in `registries` (`HolderSet.Named.bind`).
    pub fn resolve(&mut self, registries: &RegistryAccess) {
        if let ItemHolderSet::Named(tag) = &self.values {
            let members = registries.tag(keys::ITEM, tag).unwrap_or(&[]);
            self.resolved = members.iter().filter_map(|id| Item::from_id(*id)).collect();
        }
    }

    /// `items()`.
    pub fn items(&self) -> &[Item] {
        &self.resolved
    }

    /// `isEmpty()` (`values.size() == 0`).
    pub fn is_empty(&self) -> bool {
        self.resolved.is_empty()
    }

    /// `test(stack)` (`stack.is(values)`).
    pub fn test(&self, stack: &ItemStack) -> bool {
        self.accepts(stack.item())
    }

    /// `acceptsItem(item)`.
    pub fn accepts(&self, item: Item) -> bool {
        self.resolved.contains(&item)
    }

    /// `testOptionalIngredient(ingredient, stack)`.
    pub fn test_optional(ingredient: Option<&Ingredient>, stack: &ItemStack) -> bool {
        match ingredient {
            Some(i) => i.test(stack),
            None => stack.is_empty(),
        }
    }

    /// `getSingleItem()`.
    pub fn get_single_item(&self) -> Option<Item> {
        (self.resolved.len() == 1).then(|| self.resolved[0])
    }

    /// `display()`: `TagSlotDisplay(values)`.
    pub fn display(&self) -> SlotDisplay {
        SlotDisplay::Tag {
            tag: self.wire_set(),
        }
    }

    /// `optionalIngredientToDisplay`.
    pub fn optional_display(ingredient: Option<&Ingredient>) -> SlotDisplay {
        ingredient.map_or(SlotDisplay::Empty, Ingredient::display)
    }

    /// `ByteBufCodecs.holderSet(Registries.ITEM)` view of the values (what `CONTENTS_STREAM_CODEC` writes).
    pub fn wire_set(&self) -> WireHolderSet {
        match &self.values {
            ItemHolderSet::Named(tag) => WireHolderSet::Named(tag.clone()),
            ItemHolderSet::Direct(items) => {
                WireHolderSet::Direct(items.iter().map(|i| i.id() as i32).collect())
            }
        }
    }

    /// `Ingredient.CODEC`: a non-empty `NON_AIR_HOLDER_SET_CODEC` (`#tag`, one item id, or a list of item ids).
    pub fn codec() -> Codec<Ingredient> {
        let item = registry_by_name_codec::<Item>().validate(|item| {
            if item == Item::AIR {
                DataResult::error_str("Item must not be minecraft:air")
            } else {
                DataResult::success(item)
            }
        });
        let list = item.list_of();
        let direct = mc_dfu::extra::compact_list_codec_with(&item, &list);
        let either = hashed_tag_codec().either(&direct);
        let (decode_either, encode_either) = (either.clone(), either);
        let set = Codec::from_fns(
            move |ops, input: &ItemHolderSet, prefix| match input {
                ItemHolderSet::Named(tag) => {
                    encode_either.encode(ops, &Either::Left(tag.clone()), prefix)
                }
                ItemHolderSet::Direct(items) => {
                    encode_either.encode(ops, &Either::Right(items.clone()), prefix)
                }
            },
            move |ops, input| {
                decode_either
                    .decode(ops, input)
                    .flat_map(|(either, rest)| match either {
                        Either::Left(tag) => {
                            if item_tag_exists(&tag) {
                                DataResult::success((ItemHolderSet::Named(tag), rest))
                            } else {
                                let message = format!("Missing tag: '{tag}' in 'minecraft:item'");
                                DataResult::error(move || message.clone())
                            }
                        }
                        Either::Right(items) => {
                            DataResult::success((ItemHolderSet::Direct(items), rest))
                        }
                    })
            },
        );
        set.validate(|set| match &set {
            ItemHolderSet::Direct(items) if items.is_empty() => {
                DataResult::error_str("List must have contents")
            }
            _ => DataResult::success(set),
        })
        .xmap(Ingredient::from_set, |i: &Ingredient| i.values.clone())
    }
}

/// `PlacementInfo`.
#[derive(Clone, Debug, Default)]
pub struct PlacementInfo {
    ingredients: Vec<Ingredient>,
    slots_to_ingredient_index: Vec<i32>,
}

impl PlacementInfo {
    pub const EMPTY_SLOT: i32 = -1;

    /// `NOT_PLACEABLE`.
    pub fn not_placeable() -> PlacementInfo {
        PlacementInfo::default()
    }

    /// `create(ingredient)`.
    pub fn create_single(ingredient: &Ingredient) -> PlacementInfo {
        if ingredient.is_empty() {
            Self::not_placeable()
        } else {
            PlacementInfo {
                ingredients: vec![ingredient.clone()],
                slots_to_ingredient_index: vec![0],
            }
        }
    }

    /// `createFromOptionals(ingredients)`.
    pub fn create_from_optionals(ingredients: &[Option<&Ingredient>]) -> PlacementInfo {
        let mut present = Vec::with_capacity(ingredients.len());
        let mut slots = Vec::with_capacity(ingredients.len());
        let mut placement_index = 0;
        for maybe in ingredients {
            match maybe {
                Some(ingredient) => {
                    if ingredient.is_empty() {
                        return Self::not_placeable();
                    }
                    present.push((*ingredient).clone());
                    slots.push(placement_index);
                    placement_index += 1;
                }
                None => slots.push(Self::EMPTY_SLOT),
            }
        }
        PlacementInfo {
            ingredients: present,
            slots_to_ingredient_index: slots,
        }
    }

    /// `create(ingredients)`.
    pub fn create(ingredients: &[Ingredient]) -> PlacementInfo {
        if ingredients.iter().any(Ingredient::is_empty) {
            return Self::not_placeable();
        }
        PlacementInfo {
            ingredients: ingredients.to_vec(),
            slots_to_ingredient_index: (0..ingredients.len() as i32).collect(),
        }
    }

    pub fn slots_to_ingredient_index(&self) -> &[i32] {
        &self.slots_to_ingredient_index
    }

    pub fn ingredients(&self) -> &[Ingredient] {
        &self.ingredients
    }

    pub fn is_impossible_to_place(&self) -> bool {
        self.slots_to_ingredient_index.is_empty()
    }
}
