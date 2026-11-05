//! Item/experience pickup side of the player: `LivingEntity.take`, `ServerPlayer.take`, mending repair of
//! `ExperienceOrb.repairPlayerItems`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::world::entity::Entity;
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
}

/// `ExperienceOrb.repairPlayerItems(player, amount)`.
pub fn repair_player_items(_level: &mut ServerLevel, _player: &mut ServerPlayer, amount: i32) -> i32 {
    // SEAM(EcEnchant): `EnchantmentHelper.getRandomItemWith(REPAIR_WITH_XP, player, ItemStack::isDamaged)` +
    // `modifyDurabilityToRepairFromXp`; without them no item has a mending effect, so nothing is repaired.
    amount
}
