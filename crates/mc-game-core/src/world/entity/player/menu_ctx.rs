//! `MenuPlayer` over a `(ServerPlayer, ServerLevel)` pair: what menus/slots/inventory algorithms ask of the player.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::player::inventory::Inventory;
use crate::world::inventory::{
    ConnectionContainerSynchronizer, ContainerListener, ContainerSynchronizer, MenuInit,
    MenuPlayer, Prediction, Slot,
};
use crate::world::item::ItemStack;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_protocol::packets::play::ClientboundSetPlayerInventoryPacket;

pub struct PlayerCtx<'a> {
    pub player: &'a mut ServerPlayer,
    pub level: &'a mut ServerLevel,
}

impl MenuPlayer for PlayerCtx<'_> {
    fn entity_id(&self) -> i32 {
        self.player.base_id()
    }
    fn registries(&self) -> crate::registry::RegistryAccess {
        self.level.registries.clone()
    }
    fn inventory(&self) -> &Inventory {
        &self.player.player.inventory
    }
    fn inventory_mut(&mut self) -> &mut Inventory {
        &mut self.player.player.inventory
    }
    fn has_infinite_materials(&self) -> bool {
        self.player.player.abilities.instabuild
    }
    fn is_creative(&self) -> bool {
        self.player.is_creative()
    }
    fn is_spectator(&self) -> bool {
        use crate::world::entity::Entity;
        self.player.is_spectator()
    }
    fn is_removed_not_changing_dimension(&self) -> bool {
        use crate::world::entity::{Entity, RemovalReason};
        self.player
            .base()
            .removal_reason()
            .is_some_and(|r| r != RemovalReason::ChangedDimension)
    }
    fn may_build(&self) -> bool {
        self.player.player.abilities.may_build
    }
    fn is_alive(&self) -> bool {
        use crate::world::entity::Entity;
        self.player.is_alive()
    }
    fn has_disconnected(&self) -> bool {
        self.player.disconnected
    }
    /// `Player.drop(...)`: spawns the item entity (`ServerPlayer.drop`).
    fn drop(&mut self, stack: ItemStack, thrown_from_hand: bool, prediction: Prediction) {
        self.player
            .drop(self.level, stack, thrown_from_hand, prediction);
    }
    fn on_equip_item(&mut self, slot: EquipmentSlot, old: &ItemStack, new: &ItemStack) {
        crate::world::entity::Entity::on_equip_item(self.player, self.level, slot, old, new);
    }
    /// Includes `Equippable.canBeEquippedBy(player type)` (`allowed_entities`).
    fn is_equippable_in_slot(&self, stack: &ItemStack, slot: EquipmentSlot) -> bool {
        crate::world::entity::Entity::is_equippable_in_slot(&*self.player, stack, slot)
    }
    fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool {
        self.player.is_within_block_interaction_range(pos, buffer)
    }
    fn position(&self) -> mc_core::phys::Vec3 {
        crate::world::entity::Entity::position(&*self.player)
    }
    fn send_inventory_slot_update(&mut self, slot: usize, stack: ItemStack) {
        self.player.send(&ClientboundSetPlayerInventoryPacket {
            slot: slot as i32,
            contents: stack,
        });
    }
    fn play_player_sound(&mut self, sound: mc_data::SoundEvent, volume: f32, pitch: f32) {
        crate::world::item::entity_play_sound(self.level, &*self.player, sound, volume, pitch);
    }
    fn level_random_next_float(&mut self) -> f32 {
        use crate::world::level::LevelAccessor;
        self.level.random().next_float()
    }
    fn level(&mut self) -> Option<&mut ServerLevel> {
        Some(self.level)
    }
    fn container_interaction_range(&self) -> f64 {
        self.player
            .player
            .living
            .get_attribute_value(mc_data::Attribute::BLOCK_INTERACTION_RANGE)
    }
    fn is_within_entity_interaction_range(
        &self,
        target: &mc_core::phys::AABB,
        buffer: f64,
    ) -> bool {
        self.player
            .is_within_entity_interaction_range(target, buffer)
    }
    fn with_server_player(&mut self, f: &mut dyn FnMut(&mut ServerLevel, &mut ServerPlayer)) {
        f(&mut *self.level, &mut *self.player);
    }
    fn recipe_book_contains(&self, recipe_id: &mc_core::ident::Identifier) -> bool {
        self.player.recipe_book.contains(recipe_id)
    }
    fn level_ref(&self) -> Option<&ServerLevel> {
        Some(&*self.level)
    }
    fn ender_chest(&self) -> Option<&crate::world::inventory::PlayerEnderChestContainer> {
        Some(&self.player.player.ender_chest)
    }
    fn ender_chest_mut(
        &mut self,
    ) -> Option<&mut crate::world::inventory::PlayerEnderChestContainer> {
        Some(&mut self.player.player.ender_chest)
    }
    fn experience_level(&self) -> i32 {
        self.player.player.experience_level
    }
    fn give_experience_levels(&mut self, amount: i32) {
        self.player.give_experience_levels(self.level, amount);
    }
    fn enchantment_seed(&self) -> i32 {
        self.player.player.enchantment_seed
    }
    fn on_enchantment_performed(&mut self, _stack: &ItemStack, enchantment_cost: i32) {
        self.player.on_enchantment_performed(enchantment_cost);
    }
    fn on_item_enchanted(&mut self, stack: &ItemStack, enchantment_cost: i32) {
        self.player
            .award_stat_once(self.level, crate::stats::stats::Stats::ENCHANT_ITEM);
        crate::advancements::triggers::CriteriaTriggers::ENCHANTED_ITEM.trigger(
            self.level,
            self.player,
            stack,
            enchantment_cost,
        );
    }
    fn player_random_next_float(&mut self) -> f32 {
        use crate::world::entity::Entity;
        self.player.base_mut().random.next_float()
    }
}

/// `ServerPlayer.containerSynchronizer` + `containerListener`.
pub struct ServerPlayerMenuInit {
    pub connection: crate::server::network::ConnectionSender,
}

struct NoopListener;
impl ContainerListener for NoopListener {
    /// `ServerPlayer.containerListener`: the player's one listener object, attached at most once per menu.
    fn identity(&self) -> Option<usize> {
        Some(0)
    }
    /// `ServerPlayer.containerListener.slotChanged` (INVENTORY_CHANGED) runs in `AbstractContainerMenu::dispatch_slot_events`.
    fn slot_changed(&mut self, _slot: &Slot, _slot_index: usize, _stack: &ItemStack) {}
}

impl MenuInit for ServerPlayerMenuInit {
    fn container_synchronizer(&self) -> Box<dyn ContainerSynchronizer> {
        Box::new(ConnectionContainerSynchronizer::new(
            self.connection.clone(),
        ))
    }
    fn container_listener(&self) -> Box<dyn ContainerListener> {
        Box::new(NoopListener)
    }
}
