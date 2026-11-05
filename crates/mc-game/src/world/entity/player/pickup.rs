//! Item/experience pickup side of the player: `LivingEntity.take`, `ServerPlayer.take`, mending repair of
//! `ExperienceOrb.repairPlayerItems`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::world::entity::Entity;
use crate::world::entity::item::ItemEntity;
use mc_data::EntityType;
use mc_protocol::packets::play::ClientboundTakeItemEntityPacket;
use std::sync::OnceLock;

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

    /// `ServerPlayer.onItemPickup(entity)` (+ `LivingEntity.onItemPickup`): the `THROWN_ITEM_PICKED_UP_BY_ENTITY` trigger of a
    /// player owner and `THROWN_ITEM_PICKED_UP_BY_PLAYER` for this player when the item has a thrower.
    pub fn on_item_pickup(&mut self, level: &mut ServerLevel, item: &ItemEntity) {
        if let Some(hooks) = ITEM_PICKUP_HOOKS.get() {
            hooks.on_item_pickup(level, self, item);
        }
    }
}

/// SEAM(advancements): the criteria triggers of `onItemPickup`. `ItemEntity.thrower` is a UUID: when it is the picker itself the
/// picker is out of `level` (taken out while it ticks), so the implementation resolves the owner from `picker` first.
pub trait ItemPickupHooks: Send + Sync {
    fn on_item_pickup(&self, level: &mut ServerLevel, picker: &mut ServerPlayer, item: &ItemEntity);
}

static ITEM_PICKUP_HOOKS: OnceLock<&'static dyn ItemPickupHooks> = OnceLock::new();

pub fn set_item_pickup_hooks(hooks: &'static dyn ItemPickupHooks) {
    let _ = ITEM_PICKUP_HOOKS.set(hooks);
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
