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
//! The defaults implement the data-component driven base behavior of `Item` (tool, consumable, equippable swap,
//! blocks_attacks, kinetic_weapon, block_transformer hook, ...). `ItemStack`-level wrappers (`item_stack_use_on`,
//! `item_stack_use`, `finish_using_item`, `hurt_and_break`, ...) live in `use_item.rs`.
//!
//! Mutable stacks: vanilla mutates the `ItemStack` object held by the player. Behaviour methods that act on "the held
//! stack" take it as `&mut ItemStack` (a copy the caller writes back) or re-read it from the player's hand.

use super::{ClickAction, ConsumableExt, InteractionHand, InteractionResult, ItemStack, ItemStackExt, UseOnContext, consumable, tool};
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::inventory::{MenuPlayer, Slot, SlotEnv};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{DataComponentType, Item};
use mc_protocol::components::{CONSUMABLE, ITEM_NAME, ItemUseAnimation, TOOL};
use mc_text::Component;
use std::sync::OnceLock;

/// `APPROXIMATELY_INFINITE_USE_DURATION`.
pub const APPROXIMATELY_INFINITE_USE_DURATION: i32 = 72000;

pub trait ItemBehaviour: Send + Sync + 'static {
    /// `Item.useOn(context)`: the `block_transformer` component, else `PASS`. The player's held stack in
    /// `context.item_stack` may be mutated (the caller stores it back).
    fn use_on(&self, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
        super::block_transformer::use_on(context, level, player)
    }

    /// `Item.use(level, player, hand)`: consumable / swappable equippable / blocks_attacks / kinetic_weapon.
    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        consumable::base_use(level, player, hand)
    }

    /// `item instanceof DispensibleContainerItem` (buckets).
    fn as_dispensible_container(&self) -> Option<&dyn super::bucket_item::DispensibleContainerItem> {
        None
    }

    /// `Item.finishUsingItem(stack, level, entity)`.
    fn finish_using_item(&self, stack: &mut ItemStack, level: &mut ServerLevel, entity: &mut dyn Entity) -> ItemStack {
        consumable::base_finish_using_item(stack, level, entity)
    }

    /// `Item.releaseUsing(stack, level, entity, remainingTime)`: whether the release counts as a use.
    fn release_using(&self, _stack: &mut ItemStack, _level: &mut ServerLevel, _entity: &mut dyn Entity, _remaining_time: i32) -> bool {
        false
    }

    /// `Item.onUseTick(level, entity, stack, ticksRemaining)`.
    fn on_use_tick(&self, _level: &mut ServerLevel, _user: &mut dyn Entity, _stack: &mut ItemStack, _ticks_remaining: i32) {}

    /// `Item.inventoryTick(stack, level, owner, slot)`: `slot` is `None` for non-equipment inventory slots.
    fn inventory_tick(&self, _stack: &mut ItemStack, _level: &mut ServerLevel, _owner: &mut dyn Entity, _slot: Option<EquipmentSlot>) {}

    /// `Item.mineBlock(stack, level, state, pos, owner)`: tool durability.
    fn mine_block(&self, stack: &mut ItemStack, level: &mut ServerLevel, state: BlockState, pos: BlockPos, owner: &mut ServerPlayer) -> bool {
        let Some(tool) = stack.get(&TOOL).cloned() else { return false };
        let _ = pos;
        if state.get_destroy_speed() != 0.0 && tool.damage_per_block > 0 {
            super::hurt_and_break(stack, tool.damage_per_block, level, owner, EquipmentSlot::MainHand);
        }
        true
    }

    /// `Item.hurtEnemy(stack, mob, attacker)`.
    fn hurt_enemy(&self, _stack: &mut ItemStack, _mob: &mut dyn Entity, _attacker: &mut dyn Entity) {}

    /// `Item.postHurtEnemy(stack, mob, attacker)`.
    fn post_hurt_enemy(&self, _stack: &mut ItemStack, _mob: &mut dyn Entity, _attacker: &mut dyn Entity) {}

    /// `Item.interactLivingEntity(stack, player, target, hand)`.
    fn interact_living_entity(&self, _stack: &mut ItemStack, _player: &mut ServerPlayer, _target: &mut dyn Entity, _hand: InteractionHand) -> InteractionResult {
        InteractionResult::PASS
    }

    /// `Item.onDestroyed(itemEntity)`: the item entity holding `stack` was destroyed at `pos` (shulker boxes and
    /// bundles drop their contents). The caller (ItemEntity damage) writes the changed stack back.
    fn on_destroyed(&self, _level: &mut ServerLevel, _stack: &mut ItemStack, _pos: mc_core::phys::Vec3) {}

    /// `Item.canDestroyBlock(stack, state, level, pos, user)`; `stack` is the held stack (the caller stores it back).
    fn can_destroy_block(&self, stack: &mut ItemStack, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, user: &mut ServerPlayer) -> bool {
        super::tool_can_destroy_block(stack, user.player.abilities.instabuild)
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
