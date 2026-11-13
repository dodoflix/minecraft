//! `Player.interactOn(entity, hand, location)`: the player uses an item/hand on an entity.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_core::phys::Vec3;
use mc_data::GameEvent;

impl ServerPlayer {
    /// `Player.interactOn(entity, hand, location)` (`location` is relative to the entity, as sent by the client).
    pub fn interact_on(
        &mut self,
        level: &mut ServerLevel,
        target: EntityId,
        hand: InteractionHand,
        location: Vec3,
    ) -> InteractionResult {
        if self.is_spectator() {
            let provider = level
                .get_entity(target)
                .and_then(|entity| entity.as_menu_provider(level));
            if let Some(provider) = provider {
                self.open_menu(level, &*provider);
            }
            return InteractionResult::PASS;
        }
        let item_stack = self.get_item_in_hand(hand);
        let item_stack_clone = item_stack.copy();
        let Some(interact) = level.with_entity(target, |level, entity| {
            entity.interact(level, self, hand, location)
        }) else {
            return InteractionResult::PASS;
        };
        self.open_pending_mount_inventory(level);
        if interact.consumes_action() {
            // vanilla compares the stack object: the hand still holds the same (emptied or shrunk) stack
            if self.has_infinite_materials() {
                let mut restored = self.get_item_in_hand(hand);
                let shrunk = restored.count() < item_stack_clone.count();
                restored.set_count(item_stack_clone.count());
                if shrunk && restored == item_stack_clone {
                    self.set_item_in_hand(hand, restored);
                }
            }
            return interact;
        }
        if !item_stack.is_empty() {
            let infinite = self.has_infinite_materials();
            let mut stack = if infinite {
                item_stack_clone
            } else {
                self.get_item_in_hand(hand)
            };
            let result = level.with_entity(target, |level, entity| {
                entity.living()?;
                Some((
                    crate::world::item::interact_living_entity(
                        level, &mut stack, self, entity, hand,
                    ),
                    entity.position(),
                ))
            });
            if let Some(Some((result, position))) = result
                && result.consumes_action()
            {
                level.game_event_from_entity(Some(self.id()), GameEvent::ENTITY_INTERACT, position);
                if !infinite {
                    let emptied = stack.is_empty();
                    self.set_item_in_hand(hand, if emptied { ItemStack::empty() } else { stack });
                }
                return result;
            }
        }
        InteractionResult::PASS
    }
}
