//! `BundleItem` and `BundleContents.Mutable`.
//!
//! `BundleContents` (the component value) lives in the protocol crate; the mutable builder that the click handlers use
//! is here. Items inside are full `ItemStack`s (components included); weights are exact `Fraction`s.

use super::{ClickAction, InteractionHand, InteractionResult, ItemBehaviour, ItemStack, behaviour};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::inventory::{Prediction, Slot, SlotEnv};
use crate::world::level::LevelAccessor;
use mc_core::random::RandomSource;
use mc_protocol::packets::play::SoundSource;
use mc_data::SoundEvent;
use mc_protocol::components::{BUNDLE_CONTENTS, BundleContents, Fraction, ItemStackTemplate, ItemUseAnimation};

/// `BundleContents.canItemBeInBundle(stack)`.
pub fn can_item_be_in_bundle(stack: &ItemStack) -> bool {
    !stack.is_empty() && behaviour(stack.item()).can_fit_inside_container_items()
}

/// `BundleContents.getWeight(stack)`: `None` for the error case.
fn get_weight(stack: &ItemStack) -> Option<Fraction> {
    let template = ItemStackTemplate::from_stack(stack).ok()?;
    BundleContents::item_weight_fraction(&template).ok()
}

/// `BundleContents.Mutable`.
pub struct BundleMutable {
    items: Vec<ItemStack>,
    weight: Fraction,
    selected_item: i32,
}

impl BundleMutable {
    /// `BundleContents.asMutable()` (an erroring weight yields an empty builder).
    pub fn from_contents(contents: &BundleContents) -> BundleMutable {
        match contents.weight_fraction() {
            Ok(weight) => BundleMutable {
                items: contents.items().iter().map(ItemStackTemplate::create).collect(),
                weight,
                selected_item: contents.selected_item_index(),
            },
            Err(_) => BundleMutable { items: Vec::new(), weight: Fraction::ZERO, selected_item: -1 },
        }
    }

    pub fn to_immutable(&self) -> BundleContents {
        let items = self.items.iter().filter_map(|s| ItemStackTemplate::from_stack(s).ok()).collect();
        BundleContents::with_selected_item(items, self.selected_item)
    }

    fn find_stack_index(&self, stack: &ItemStack) -> Option<usize> {
        if !stack.is_stackable() {
            return None;
        }
        self.items.iter().position(|i| ItemStack::is_same_item_same_components(i, stack))
    }

    fn get_max_amount_to_add(&self, item_weight: Fraction) -> i32 {
        let remaining = Fraction::ONE.subtract(self.weight).unwrap_or(Fraction::ZERO);
        remaining.divide_by(item_weight).map_or(0, Fraction::int_value).max(0)
    }

    fn stacked_weight(weight: Fraction, count: i32) -> Fraction {
        weight.multiply_by(Fraction::get(count, 1).unwrap_or(Fraction::ZERO)).unwrap_or(Fraction::ZERO)
    }

    /// `tryInsert(itemsToAdd)`: moves what fits out of `to_add`; returns the amount moved.
    pub fn try_insert(&mut self, to_add: &mut ItemStack) -> i32 {
        if !can_item_be_in_bundle(to_add) {
            return 0;
        }
        let Some(item_weight) = get_weight(to_add) else { return 0 };
        let amount = to_add.count().min(self.get_max_amount_to_add(item_weight));
        if amount == 0 {
            return 0;
        }
        self.weight = self.weight.add(Self::stacked_weight(item_weight, amount)).unwrap_or(self.weight);
        match self.find_stack_index(to_add) {
            Some(i) => {
                let removed = self.items.remove(i);
                let merged = removed.copy_with_count(removed.count() + amount);
                to_add.shrink(amount);
                self.items.insert(0, merged);
            }
            None => {
                let part = to_add.split(amount);
                self.items.insert(0, part);
            }
        }
        amount
    }

    /// `tryTransfer(slot, player)`.
    pub fn try_transfer(&mut self, slot: &Slot, env: &mut SlotEnv) -> i32 {
        let other = slot.get_item(env);
        let Some(item_weight) = get_weight(&other) else { return 0 };
        let max_amount = self.get_max_amount_to_add(item_weight);
        if can_item_be_in_bundle(&other) {
            let mut taken = slot.safe_take(env, other.count(), max_amount);
            self.try_insert(&mut taken)
        } else {
            0
        }
    }

    fn index_is_outside_allowed_bounds(&self, selected: i32) -> bool {
        selected < 0 || selected as usize >= self.items.len()
    }

    /// `toggleSelectedItem(selectedItem)`.
    pub fn toggle_selected_item(&mut self, selected: i32) {
        self.selected_item = if self.selected_item != selected && !self.index_is_outside_allowed_bounds(selected) { selected } else { -1 };
    }

    /// `removeOne()`.
    pub fn remove_one(&mut self) -> Option<ItemStack> {
        if self.items.is_empty() {
            return None;
        }
        let remove_index = if self.index_is_outside_allowed_bounds(self.selected_item) { 0 } else { self.selected_item as usize };
        let stack = self.items.remove(remove_index).copy();
        if let Some(w) = get_weight(&stack) {
            self.weight = self.weight.subtract(Self::stacked_weight(w, stack.count())).unwrap_or(self.weight);
        }
        self.toggle_selected_item(-1);
        Some(stack)
    }
}

/// `BundleItem.toggleSelectedItem(stack, selectedItem)` (`ServerboundSelectBundleItemPacket`).
pub fn toggle_selected_item(stack: &mut ItemStack, selected_item: i32) {
    let Some(contents) = stack.get(&BUNDLE_CONTENTS).cloned() else { return };
    let mut m = BundleMutable::from_contents(&contents);
    m.toggle_selected_item(selected_item);
    stack.set(BUNDLE_CONTENTS, m.to_immutable());
}

fn play_insert_sound(env: &mut SlotEnv) {
    let pitch = 0.8 + env.player.level_random_next_float() * 0.4;
    env.player.play_player_sound(SoundEvent::ITEM_BUNDLE_INSERT, 0.8, pitch);
}
fn play_insert_fail_sound(env: &mut SlotEnv) {
    env.player.play_player_sound(SoundEvent::ITEM_BUNDLE_INSERT_FAIL, 1.0, 1.0);
}
fn play_remove_one_sound(env: &mut SlotEnv) {
    let pitch = 0.8 + env.player.level_random_next_float() * 0.4;
    env.player.play_player_sound(SoundEvent::ITEM_BUNDLE_REMOVE_ONE, 0.8, pitch);
}

pub struct BundleItem;

impl ItemBehaviour for BundleItem {
    fn override_stacked_on_other(&self, me: &mut ItemStack, slot: &Slot, env: &mut SlotEnv, action: ClickAction) -> bool {
        let Some(initial) = me.get(&BUNDLE_CONTENTS).cloned() else { return false };
        let other = slot.get_item(env);
        let mut contents = BundleMutable::from_contents(&initial);
        if action == ClickAction::Primary && !other.is_empty() {
            if contents.try_transfer(slot, env) > 0 {
                play_insert_sound(env);
            } else {
                play_insert_fail_sound(env);
            }
            me.set(BUNDLE_CONTENTS, contents.to_immutable());
            *env.slots_changed_requested = true;
            true
        } else if action == ClickAction::Secondary && other.is_empty() {
            if let Some(item) = contents.remove_one() {
                let mut remainder = slot.safe_insert(env, item);
                if remainder.count() > 0 {
                    contents.try_insert(&mut remainder);
                } else {
                    play_remove_one_sound(env);
                }
            }
            me.set(BUNDLE_CONTENTS, contents.to_immutable());
            *env.slots_changed_requested = true;
            true
        } else {
            false
        }
    }

    fn override_other_stacked_on_me(&self, me: &mut ItemStack, other: &mut ItemStack, slot: &Slot, env: &mut SlotEnv, action: ClickAction) -> bool {
        if action == ClickAction::Primary && other.is_empty() {
            toggle_selected_item(me, -1);
            return false;
        }
        let Some(initial) = me.get(&BUNDLE_CONTENTS).cloned() else { return false };
        let mut contents = BundleMutable::from_contents(&initial);
        if action == ClickAction::Primary && !other.is_empty() {
            if slot.allow_modification(env) && contents.try_insert(other) > 0 {
                play_insert_sound(env);
            } else {
                play_insert_fail_sound(env);
            }
            me.set(BUNDLE_CONTENTS, contents.to_immutable());
            *env.slots_changed_requested = true;
            true
        } else if action == ClickAction::Secondary && other.is_empty() {
            if slot.allow_modification(env) {
                if let Some(removed) = contents.remove_one() {
                    play_remove_one_sound(env);
                    *other = removed;
                }
            }
            me.set(BUNDLE_CONTENTS, contents.to_immutable());
            *env.slots_changed_requested = true;
            true
        } else {
            toggle_selected_item(me, -1);
            false
        }
    }

    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        player.start_using_item(level, hand);
        InteractionResult::SUCCESS
    }

    fn on_use_tick(&self, level: &mut ServerLevel, user: &mut dyn Entity, stack: &mut ItemStack, ticks_remaining: i32) {
        let Some(player) = user.as_any_mut().downcast_mut::<ServerPlayer>() else { return };
        let use_duration = self.get_use_duration(stack);
        let is_first_tick = ticks_remaining == use_duration;
        if is_first_tick || (ticks_remaining < use_duration - 10 && ticks_remaining % 2 == 0) {
            drop_content_and_sound(level, player, stack);
        }
    }

    fn get_use_duration(&self, _stack: &ItemStack) -> i32 {
        200
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Bundle
    }
}

register_item_class!("BundleItem", |_| Box::new(BundleItem));

/// `BundleItem.dropContent(level, player, stack)`.
fn drop_content_and_sound(level: &mut ServerLevel, player: &mut ServerPlayer, bundle: &mut ItemStack) {
    let Some(contents) = bundle.get(&BUNDLE_CONTENTS).cloned() else { return };
    if contents.is_empty() {
        return;
    }
    let mut m = BundleMutable::from_contents(&contents);
    let Some(removed) = m.remove_one() else { return };
    // `playRemoveOneSound(player)`, then the stack is updated and dropped
    let pitch = 0.8 + level.random().next_float() * 0.4;
    crate::world::item::entity_play_sound(level, &*player, SoundEvent::ITEM_BUNDLE_REMOVE_ONE, 0.8, pitch);
    bundle.set(BUNDLE_CONTENTS, m.to_immutable());
    player.drop(level, removed, true, Prediction::Predicted);
    // `playDropContentsSound(level, player)`
    let pitch = 0.8 + level.random().next_float() * 0.4;
    let pos = player.base().block_position();
    level.play_sound(None, pos, SoundEvent::ITEM_BUNDLE_DROP_CONTENTS, SoundSource::Players, 0.8, pitch);
}
