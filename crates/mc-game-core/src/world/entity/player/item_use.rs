//! The item-facing part of `LivingEntity`/`Player` for a `ServerPlayer`: `setItemSlot`/`onEquipItem`, the using-item
//! state machine (`startUsingItem`, `updatingUsingItem`, `completeUsingItem`, `releaseUsingItem`, `stopUsingItem`),
//! `swing`, item cooldowns, `canEat`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::server::level::level_events::encode_packet;
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living::{LIVING_ENTITY_FLAG_IS_USING, LIVING_ENTITY_FLAG_OFF_HAND};
use crate::world::item::{InteractionHand, ItemStack, behaviour};
use mc_data::{DataComponentType, GameEvent};
use mc_protocol::components::USE_EFFECTS;
use mc_protocol::packets::play::{
    ClientboundCooldownPacket, ClientboundSetEquipmentPacket, SwingAnimation, SwingAnimationType,
};

/// `SwingAnimation.DEFAULT`.
pub const SWING_ANIMATION_DEFAULT: SwingAnimation = SwingAnimation {
    kind: SwingAnimationType::Whack,
    duration: 6,
};

impl ServerPlayer {
    /// `isUsingItem()`.
    pub fn is_using_item(&self) -> bool {
        self.player
            .living
            .get_living_entity_flag(LIVING_ENTITY_FLAG_IS_USING)
    }

    /// `getUsedItemHand()`.
    pub fn get_used_item_hand(&self) -> InteractionHand {
        if self
            .player
            .living
            .get_living_entity_flag(LIVING_ENTITY_FLAG_OFF_HAND)
        {
            InteractionHand::OffHand
        } else {
            InteractionHand::MainHand
        }
    }

    /// `getUseItem()` (a copy of the stack in use).
    pub fn get_use_item(&self) -> &ItemStack {
        &self.player.living.use_item
    }

    pub fn get_use_item_remaining_ticks(&self) -> i32 {
        self.player.living.use_item_remaining
    }

    /// `getTicksUsingItem()`.
    pub fn get_ticks_using_item(&self) -> i32 {
        if self.is_using_item() {
            behaviour(self.player.living.use_item.item())
                .get_use_duration(&self.player.living.use_item)
                - self.player.living.use_item_remaining
        } else {
            0
        }
    }

    /// `ItemStack.causeUseVibration(causer, event)`.
    pub(crate) fn cause_use_vibration(
        &self,
        level: &mut ServerLevel,
        stack: &ItemStack,
        event: GameEvent,
    ) {
        if stack
            .get(&USE_EFFECTS)
            .is_some_and(|e| e.interact_vibrations)
        {
            level.game_event_from_entity_ref(self, event, self.base().position());
        }
    }

    /// `LivingEntity.updatePlayersWithNewEquipment`: `ClientboundSetEquipmentPacket` to the players tracking this one.
    fn update_players_with_new_equipment(
        &self,
        level: &mut ServerLevel,
        slot: EquipmentSlot,
        stack: &ItemStack,
    ) {
        let packet = ClientboundSetEquipmentPacket {
            entity: self.base_id(),
            slots: vec![(slot.to_protocol(), stack.clone())],
        };
        level
            .entity_tracker
            .send_to_tracking_players(self.base_id(), &encode_packet(&packet));
    }

    /// `startUsingItem(hand)`.
    pub fn start_using_item(&mut self, level: &mut ServerLevel, hand: InteractionHand) {
        let stack = self.get_item_in_hand(hand);
        if !stack.is_empty() && !self.is_using_item() {
            self.player.living.use_item = stack.clone();
            self.player.living.use_item_remaining =
                behaviour(stack.item()).get_use_duration(&stack);
            self.update_players_with_new_equipment(level, hand.as_equipment_slot(), &stack);
            self.player
                .living
                .set_living_entity_flag(LIVING_ENTITY_FLAG_IS_USING, true);
            self.player.living.set_living_entity_flag(
                LIVING_ENTITY_FLAG_OFF_HAND,
                hand == InteractionHand::OffHand,
            );
            self.cause_use_vibration(level, &stack, GameEvent::ITEM_INTERACT_START);
            if stack.has(DataComponentType::KINETIC_WEAPON) {
                self.player.living.recent_kinetic_enemies = Some(Default::default());
            }
        }
    }

    /// `stopUsingItem()`.
    pub fn stop_using_item(&mut self, level: &mut ServerLevel) {
        let was_using = self.is_using_item();
        self.player.living.recent_kinetic_enemies = None;
        self.player
            .living
            .set_living_entity_flag(LIVING_ENTITY_FLAG_IS_USING, false);
        if was_using {
            let stack = self.player.living.use_item.clone();
            self.cause_use_vibration(level, &stack, GameEvent::ITEM_INTERACT_FINISH);
        }
        self.player.living.use_item = ItemStack::empty();
        self.player.living.use_item_remaining = 0;
    }

    /// `releaseUsingItem()`.
    pub fn release_using_item(&mut self, level: &mut ServerLevel) {
        let hand = self.get_used_item_hand();
        let in_hand = self.get_item_in_hand(hand);
        if !self.player.living.use_item.is_empty()
            && ItemStack::is_same_item(&in_hand, &self.player.living.use_item)
        {
            let remaining = self.player.living.use_item_remaining;
            let mut stack = in_hand;
            self.player.living.use_item = stack.clone();
            let before = stack.copy();
            let released =
                behaviour(stack.item()).release_using(&mut stack, level, self, remaining);
            if released {
                let with_side_effects = crate::world::item::apply_after_use_component_side_effects(
                    level,
                    stack.clone(),
                    self,
                    &before,
                );
                self.set_item_in_hand_slot(level, hand, with_side_effects);
            } else {
                self.set_item_in_hand_slot(level, hand, stack.clone());
            }
            self.player.living.use_item = self.get_item_in_hand(hand);
            let use_on_release = behaviour(self.player.living.use_item.item())
                .use_on_release(&self.player.living.use_item);
            if use_on_release {
                self.updating_using_item(level);
            }
        }
        self.stop_using_item(level);
    }

    /// `updatingUsingItem()` (`LivingEntity.tick`).
    pub fn updating_using_item(&mut self, level: &mut ServerLevel) {
        if self.is_using_item() {
            let hand = self.get_used_item_hand();
            let in_hand = self.get_item_in_hand(hand);
            if ItemStack::is_same_item(&in_hand, &self.player.living.use_item) {
                self.player.living.use_item = in_hand;
                self.update_using_item(level);
            } else {
                self.stop_using_item(level);
            }
        }
    }

    /// `updateUsingItem(useItem)`.
    fn update_using_item(&mut self, level: &mut ServerLevel) {
        let mut stack = self.player.living.use_item.clone();
        crate::advancements::triggers::CriteriaTriggers::USING_ITEM.trigger(level, self, &stack);
        let remaining = self.player.living.use_item_remaining;
        crate::world::item::on_use_tick(&mut stack, level, self, remaining);
        // the hook may have changed the stack (damage, charge): vanilla mutates the shared object
        let hand = self.get_used_item_hand();
        if stack != self.get_item_in_hand(hand) {
            self.set_item_in_hand(hand, stack.clone());
        }
        // the hook may have released the item (`stopUsingItem` cleared `useItem`)
        if self.is_using_item() {
            self.player.living.use_item = stack.clone();
        }
        self.player.living.use_item_remaining -= 1;
        if self.player.living.use_item_remaining == 0
            && !behaviour(stack.item()).use_on_release(&stack)
        {
            self.complete_using_item(level);
        }
    }

    /// `completeUsingItem()`.
    pub fn complete_using_item(&mut self, level: &mut ServerLevel) {
        let hand = self.get_used_item_hand();
        if self.player.living.use_item != self.get_item_in_hand(hand) {
            self.release_using_item(level);
        } else if !self.player.living.use_item.is_empty() && self.is_using_item() {
            let mut stack = self.player.living.use_item.clone();
            let result = crate::world::item::finish_using_item(&mut stack, level, self);
            if result != self.get_item_in_hand(hand) || stack != self.get_item_in_hand(hand) {
                self.set_item_slot(level, hand.as_equipment_slot(), result);
            }
            self.stop_using_item(level);
        }
    }

    /// `setItemInHand(hand, stack)` = `setItemSlot(slot, stack)` with `onEquipItem`.
    pub fn set_item_in_hand_slot(
        &mut self,
        level: &mut ServerLevel,
        hand: InteractionHand,
        stack: ItemStack,
    ) {
        self.set_item_slot(level, hand.as_equipment_slot(), stack);
    }

    /// `LivingEntity.setItemSlot(slot, stack)`.
    pub fn set_item_slot(
        &mut self,
        level: &mut ServerLevel,
        slot: EquipmentSlot,
        stack: ItemStack,
    ) {
        let old = self.player.inventory.equipment_set(slot, stack.clone());
        self.on_equip_item(level, slot, &old, &stack);
    }

    /// `doCloseContainer()`.
    pub fn do_close_container(&mut self, level: &mut ServerLevel) {
        use super::menu_ctx::PlayerCtx;
        let mut ctx = PlayerCtx {
            player: self,
            level,
        };
        let mut menus = std::mem::take(&mut ctx.player.player.menus);
        menus.do_close_container(&mut ctx);
        ctx.player.player.menus = menus;
    }

    /// `swing(hand, animation, sendToSwingingEntity)` (`LivingEntity.swing`).
    pub fn swing(
        &mut self,
        level: &mut ServerLevel,
        hand: InteractionHand,
        animation: SwingAnimation,
        send_to_swinging_entity: bool,
    ) -> bool {
        crate::world::entity::living_entity::swing::swing(
            self,
            level,
            hand,
            animation,
            send_to_swinging_entity,
        )
    }

    /// `Inventory.tick()`: `Item.inventoryTick` for every inventory stack (taken out of its slot while it runs;
    /// `popTime` is client-only state and not modelled).
    pub fn tick_inventory_items(&mut self, level: &mut ServerLevel) {
        let selected = self.player.inventory.get_selected_slot() as usize;
        for i in 0..self.player.inventory.get_non_equipment_items().len() {
            let mut stack =
                std::mem::take(&mut self.player.inventory.get_non_equipment_items_mut()[i]);
            if !stack.is_empty() {
                let slot = if i == selected {
                    Some(EquipmentSlot::MainHand)
                } else {
                    None
                };
                behaviour(stack.item()).inventory_tick(&mut stack, level, self, slot);
            }
            self.player.inventory.get_non_equipment_items_mut()[i] = stack;
        }
    }

    /// `EntityEquipment.tick(owner)`: `Item.inventoryTick` for the equipped stacks.
    pub fn tick_equipment_items(&mut self, level: &mut ServerLevel) {
        for slot in EquipmentSlot::VALUES {
            if slot == EquipmentSlot::MainHand {
                continue;
            }
            let mut stack = std::mem::take(self.player.inventory.equipment_mut().get_mut(slot));
            if !stack.is_empty() {
                behaviour(stack.item()).inventory_tick(&mut stack, level, self, Some(slot));
            }
            *self.player.inventory.equipment_mut().get_mut(slot) = stack;
        }
    }

    /// `ServerItemCooldowns`: sends the queued `ClientboundCooldownPacket`s.
    pub fn flush_item_cooldown_packets(&mut self) {
        for (group, duration) in self.player.cooldowns.take_outbox() {
            self.send(&ClientboundCooldownPacket {
                cooldown_group: group,
                duration,
            });
        }
    }

    /// `Player.canEat(canAlwaysEat)`.
    pub fn can_eat(&self, can_always_eat: bool) -> bool {
        self.player.abilities.invulnerable || can_always_eat || self.player.food_data.needs_food()
    }
}
