//! `ItemBehaviour`: vanilla `Item` (and its subclasses) overridable logic as a trait with vanilla defaults, plus the
//! registry mapping every `Item` id to its static implementor — exactly parallel to
//! `world::level::block::BlockBehaviour`.
//!
//! **Static data** (max stack size, default components, crafting remainder, ...) is NOT here: it comes from
//! `mc_data::Item::data()`. Each Java `Item` subclass with logic becomes one Rust type implementing the trait,
//! registered with [`register_item_class!`] (every item whose vanilla Java class has that simple name; the lookup
//! walks the class chain `ItemData::classes`) or [`register_item!`] (one specific item, overriding class
//! registrations), from the module that implements it. Items without a registration use [`PlainItem`] = vanilla's
//! base `Item`.
//!
//! The defaults implement the data-component driven base behavior of `Item` (tool, consumable, blocks_attacks,
//! kinetic_weapon, ...). Methods that need entity/level APIs that do not exist yet (`use`, `useOn`, `hurtEnemy`,
//! `mineBlock`, `inventoryTick`, `interactLivingEntity`, `finishUsingItem`, `releaseUsing`, `onUseTick`, `onDestroyed`)
//! are added by the item-behaviour wave together with the player/entity API they take; every method has a default so
//! adding one never touches existing implementors.

use super::{ClickAction, ItemStack, tool};
use super::ItemStackExt;
use crate::world::inventory::{MenuPlayer, Slot, SlotEnv};
use linkme::distributed_slice;
use mc_data::Item;
use mc_data::block::BlockState;
use mc_text::Component;
use std::sync::OnceLock;

/// `ItemUseAnimation`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemUseAnimation {
    None,
    Eat,
    Drink,
    Block,
    Bow,
    Spear,
    Crossbow,
    Spyglass,
    TootHorn,
    Brush,
    Bundle,
}

impl ItemUseAnimation {
    /// `StringRepresentable` names (the `consumable.animation` codec).
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "eat" => Self::Eat,
            "drink" => Self::Drink,
            "block" => Self::Block,
            "bow" => Self::Bow,
            "spear" => Self::Spear,
            "crossbow" => Self::Crossbow,
            "spyglass" => Self::Spyglass,
            "toot_horn" => Self::TootHorn,
            "brush" => Self::Brush,
            "bundle" => Self::Bundle,
            _ => return None,
        })
    }
}

/// `APPROXIMATELY_INFINITE_USE_DURATION`.
pub const APPROXIMATELY_INFINITE_USE_DURATION: i32 = 72000;

pub trait ItemBehaviour: Send + Sync + 'static {
    /// `getDestroySpeed(stack, state)`.
    fn get_destroy_speed(&self, stack: &ItemStack, state: BlockState) -> f32 {
        tool::get_mining_speed(stack, state)
    }

    /// `isCorrectToolForDrops(stack, state)`.
    fn is_correct_tool_for_drops(&self, stack: &ItemStack, state: BlockState) -> bool {
        tool::is_correct_for_drops(stack, state)
    }

    /// `getUseAnimation(stack)`.
    fn get_use_animation(&self, stack: &ItemStack) -> ItemUseAnimation {
        if let Some(consumable) = stack.get_json("minecraft:consumable") {
            consumable.get("animation").and_then(|a| a.as_str()).and_then(ItemUseAnimation::by_name).unwrap_or(ItemUseAnimation::Eat)
        } else if stack.has_json("minecraft:blocks_attacks") {
            ItemUseAnimation::Block
        } else if stack.has_json("minecraft:kinetic_weapon") {
            ItemUseAnimation::Spear
        } else {
            ItemUseAnimation::None
        }
    }

    /// `getUseDuration(stack, user)`.
    fn get_use_duration(&self, stack: &ItemStack) -> i32 {
        if let Some(consumable) = stack.get_json("minecraft:consumable") {
            // `Consumable.consumeTicks()` = `(int)(consumeSeconds * 20.0F)`, default 1.6 seconds.
            let seconds = consumable.get("consume_seconds").and_then(|s| s.as_f64()).map_or(1.6f32, |s| s as f32);
            (seconds * 20.0f32) as i32
        } else if !stack.has_json("minecraft:blocks_attacks") && !stack.has_json("minecraft:kinetic_weapon") {
            0
        } else {
            APPROXIMATELY_INFINITE_USE_DURATION
        }
    }

    /// `overrideStackedOnOther(self, slot, clickAction, player)`: `me` is the carried stack stacked onto `slot`.
    fn override_stacked_on_other(&self, _me: &mut ItemStack, _slot: &Slot, _env: &mut SlotEnv, _action: ClickAction) -> bool {
        false
    }

    /// `overrideOtherStackedOnMe(self, other, slot, clickAction, player, carriedItem)`: `me` is the stack in `slot`
    /// (written back by the caller), `carried` is `other` and doubles as the `SlotAccess` for the cursor.
    fn override_other_stacked_on_me(&self, _me: &mut ItemStack, _carried: &mut ItemStack, _slot: &Slot, _env: &mut SlotEnv, _action: ClickAction) -> bool {
        false
    }

    /// `onCraftedBy(stack, player)`.
    fn on_crafted_by(&self, stack: &mut ItemStack, _player: &mut dyn MenuPlayer) {
        self.on_crafted_post_process(stack);
    }

    /// `onCraftedPostProcess(stack, level)`.
    fn on_crafted_post_process(&self, _stack: &mut ItemStack) {}

    /// `getName(stack)`: the `item_name` component.
    fn get_name(&self, stack: &ItemStack) -> Component {
        stack.get_json("minecraft:item_name").and_then(|v| Component::from_json(v).ok()).unwrap_or_else(|| Component::literal(""))
    }

    /// `isFoil(stack)`.
    fn is_foil(&self, stack: &ItemStack) -> bool {
        stack.is_enchanted()
    }

    /// `useOnRelease(stack)`.
    fn use_on_release(&self, _stack: &ItemStack) -> bool {
        false
    }

    /// `canFitInsideContainerItems()`.
    fn can_fit_inside_container_items(&self) -> bool {
        true
    }

    /// `getAttackDamageBonus(victim, damage, damageSource)`.
    fn get_attack_damage_bonus(&self, _damage: f32) -> f32 {
        0.0
    }
}

/// Vanilla's base `Item` (every method default).
pub struct PlainItem;
impl ItemBehaviour for PlainItem {}

/// Registers behavior for every item whose Java class simple name is `class`.
pub struct ItemClassRegistration {
    pub class: &'static str,
    pub create: fn(Item) -> Box<dyn ItemBehaviour>,
}
/// Registers behavior for one item by its registry name (e.g. `minecraft:bundle`), overriding class registrations.
pub struct ItemRegistration {
    pub item: &'static str,
    pub create: fn(Item) -> Box<dyn ItemBehaviour>,
}

#[distributed_slice]
pub static ITEM_CLASSES: [ItemClassRegistration];
#[distributed_slice]
pub static ITEMS: [ItemRegistration];

/// `register_item_class!("BundleItem", |item| Box::new(BundleItem))`.
#[macro_export]
macro_rules! register_item_class {
    ($class:literal, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::item::ITEM_CLASSES)]
            static REG: $crate::world::item::ItemClassRegistration = $crate::world::item::ItemClassRegistration { class: $class, create: $create };
        };
    };
}
/// `register_item!("minecraft:bundle", |item| Box::new(BundleItem))`.
#[macro_export]
macro_rules! register_item {
    ($name:literal, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::item::ITEMS)]
            static REG: $crate::world::item::ItemRegistration = $crate::world::item::ItemRegistration { item: $name, create: $create };
        };
    };
}

static TABLE: OnceLock<Vec<Box<dyn ItemBehaviour>>> = OnceLock::new();

fn build(item: Item) -> Box<dyn ItemBehaviour> {
    let data = item.data();
    if let Some(r) = ITEMS.iter().find(|r| r.item == data.name) {
        return (r.create)(item);
    }
    for class in data.classes {
        if let Some(r) = ITEM_CLASSES.iter().find(|r| r.class == *class) {
            return (r.create)(item);
        }
    }
    Box::new(PlainItem)
}

/// The static behavior implementor of `item` (`stack.getItem()` + virtual dispatch in vanilla).
pub fn behaviour(item: Item) -> &'static dyn ItemBehaviour {
    TABLE.get_or_init(|| Item::iter().map(build).collect())[item.id() as usize].as_ref()
}
