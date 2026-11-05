//! Item/experience pickup side of the player: `LivingEntity.take`, `ServerPlayer.take`, mending repair of
//! `ExperienceOrb.repairPlayerItems`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::server::level::server_entity::encode_payload;
use crate::world::entity::Entity;
use crate::world::entity::item::ItemEntity;
use mc_data::EntityType;
use mc_protocol::packets::play::ClientboundTakeItemEntityPacket;

impl ServerPlayer {
    /// `ServerPlayer.take(entity, orgCount)` = `LivingEntity.take` + `containerMenu.broadcastChanges()`: the pickup
    /// animation of items, arrows and experience orbs to the players tracking `entity`.
    pub fn take(&mut self, level: &mut ServerLevel, entity: &dyn Entity, org_count: i32) {
        let ty = entity.get_type();
        let pickable = ty == EntityType::ITEM || ty == EntityType::EXPERIENCE_ORB || ty == EntityType::ARROW || ty == EntityType::SPECTRAL_ARROW || ty == EntityType::TRIDENT;
        if !entity.is_removed() && pickable {
            let packet = ClientboundTakeItemEntityPacket { item_id: entity.id(), player_id: self.base_id(), amount: org_count };
            level.entity_tracker.send_to_tracking_players(entity.id(), &encode_payload(&packet));
        }
        self.broadcast_menu_changes(level);
    }

    /// `ServerPlayer.onItemPickup(entity)`: `LivingEntity.onItemPickup` (`THROWN_ITEM_PICKED_UP_BY_ENTITY` for a player thrower),
    /// then `THROWN_ITEM_PICKED_UP_BY_PLAYER` for any thrower. `ItemEntity.thrower` is a UUID: when it is the picker itself
    /// the picker is out of `level` (taken out while it ticks), so it is resolved from `self` first.
    pub fn on_item_pickup(&mut self, level: &mut ServerLevel, item: &ItemEntity) {
        use crate::advancements::triggers::CriteriaTriggers;
        let Some(thrower) = item.thrower else { return };
        let stack = item.get_item();
        let stack = &stack;
        if thrower == self.uuid() {
            let matches = CriteriaTriggers::THROWN_ITEM_PICKED_UP_BY_ENTITY.matches(&*level, &*self, stack, &*self);
            matches.award(level, self);
            let matches = CriteriaTriggers::THROWN_ITEM_PICKED_UP_BY_PLAYER.matches(&*level, &*self, stack, &*self);
            matches.award(level, self);
            return;
        }
        living_on_item_pickup(level, &*self, Some(thrower), stack);
        if let Some(owner) = level.get_entity_by_uuid(thrower) {
            let matches = CriteriaTriggers::THROWN_ITEM_PICKED_UP_BY_PLAYER.matches(&*level, &*self, stack, owner);
            matches.award(level, self);
        }
    }
}

/// `LivingEntity.onItemPickup(entity)`: `THROWN_ITEM_PICKED_UP_BY_ENTITY.trigger(owner, item, this)` when the item's thrower
/// (`getOwner()`) is a `ServerPlayer` in the level.
pub fn living_on_item_pickup(level: &mut ServerLevel, picker: &dyn Entity, thrower: Option<uuid::Uuid>, stack: &ItemStack) {
    let Some(owner) = thrower.and_then(|uuid| level.get_entity_by_uuid(uuid)).filter(|e| e.is_player()).map(|e| e.id()) else { return };
    level.with_player(owner, |level, owner| {
        crate::advancements::triggers::CriteriaTriggers::THROWN_ITEM_PICKED_UP_BY_ENTITY.trigger(level, owner, stack, picker);
    });
}

/// `mob.onItemPickup(itemEntity)` for the item entity `item_id` (a mob's `pickUpItem`, before the item is taken).
pub fn on_item_pickup_by_id(level: &mut ServerLevel, picker: &dyn Entity, item_id: crate::world::entity::EntityId) {
    let Some((thrower, stack)) = level.get_entity(item_id).and_then(|e| e.as_any().downcast_ref::<crate::world::entity::item::ItemEntity>()).map(|i| (i.thrower, i.get_item())) else {
        return;
    };
    living_on_item_pickup(level, picker, thrower, &stack);
}

/// `ExperienceOrb.repairPlayerItems(player, amount)`: mending. Returns the experience that is left over.
pub fn repair_player_items(level: &mut ServerLevel, player: &mut ServerPlayer, amount: i32) -> i32 {
    use crate::world::item::enchantment::actors::Actors;
    use crate::world::item::enchantment::enchantment_effect_components::EffectComponent;
    use crate::world::item::enchantment_helper::{get_random_item_with, modify_durability_to_repair_from_xp};
    let player_id = player.base_id();
    let selected = get_random_item_with(level, &mut Actors::of(player), EffectComponent::repair_with_xp, player_id, |s| s.is_damaged());
    let Some(selected) = selected else { return amount };
    let to_repair_from_xp = modify_durability_to_repair_from_xp(level, &selected.item_stack, amount);
    let damage = selected.item_stack.get_damage_value();
    let repair = to_repair_from_xp.min(damage);
    // `itemStack.setDamageValue(...)` on the live stack of the slot
    if let Some(stack) = selected.in_slot.and_then(|slot| player.item_by_slot_mut(slot)) {
        stack.set_damage_value(damage - repair);
    }
    if repair > 0 {
        let remaining = amount.wrapping_sub(repair.wrapping_mul(amount) / to_repair_from_xp);
        if remaining > 0 {
            return repair_player_items(level, player, remaining);
        }
    }
    0
}
