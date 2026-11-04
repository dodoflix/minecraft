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

use super::{ClickAction, ConsumableExt, ItemStack, ItemStackExt, tool};
use mc_protocol::components::{CONSUMABLE, ITEM_NAME, ItemUseAnimation};
use crate::world::inventory::{MenuPlayer, Slot, SlotEnv};
use linkme::distributed_slice;
use mc_data::{DataComponentType, Item};
use mc_data::block::BlockState;
use mc_text::Component;
use std::sync::OnceLock;

/// `APPROXIMATELY_INFINITE_USE_DURATION`.
pub const APPROXIMATELY_INFINITE_USE_DURATION: i32 = 72000;

pub trait ItemBehaviour: Send + Sync + 'static {
    /// `Item.useOn(context)`: default `PASS`. The player's held stack in `context.item_stack` may be mutated.
    fn use_on(&self, _context: &mut super::UseOnContext, _level: &mut crate::server::level::ServerLevel, _player: &mut crate::world::entity::ServerPlayer) -> super::InteractionResult {
        super::InteractionResult::PASS
    }

    /// `Item.use(level, player, hand)`: default `PASS` (the consumable/equippable component behavior is added by GameItems).
    fn use_item(&self, _level: &mut crate::server::level::ServerLevel, _player: &mut crate::world::entity::ServerPlayer, _hand: super::InteractionHand) -> super::InteractionResult {
        super::InteractionResult::PASS
    }

    /// `Item.canDestroyBlock(stack, state, level, pos, user)`.
    fn can_destroy_block(&self, stack: &ItemStack, _state: BlockState, user_is_creative: bool) -> bool {
        match stack.get_json("minecraft:tool") {
            Some(t) if !t.get("can_destroy_blocks_in_creative").and_then(|v| v.as_bool()).unwrap_or(true) => !user_is_creative,
            _ => true,
        }
    }

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
        if let Some(consumable) = stack.get(&CONSUMABLE) {
            consumable.animation
        } else if stack.has(DataComponentType::BLOCKS_ATTACKS) {
            ItemUseAnimation::Block
        } else if stack.has(DataComponentType::KINETIC_WEAPON) {
            ItemUseAnimation::Spear
        } else {
            ItemUseAnimation::None
        }
    }

    /// `getUseDuration(stack, user)`.
    fn get_use_duration(&self, stack: &ItemStack) -> i32 {
        if let Some(consumable) = stack.get(&CONSUMABLE) {
            consumable.consume_ticks()
        } else if !stack.has(DataComponentType::BLOCKS_ATTACKS) && !stack.has(DataComponentType::KINETIC_WEAPON) {
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
        stack.get(&ITEM_NAME).cloned().unwrap_or_else(|| Component::literal(""))
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
