//! `MenuPlayer`: the part of vanilla's `Player`/`ServerPlayer`/`LivingEntity` that menus, slots and the inventory
//! algorithms consult. Implemented by the real player context (level + player) and by test players.

use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::player::inventory::Inventory;
use crate::world::item::ItemStack;
use crate::world::item::crafting::RecipeHolder;
use mc_core::pos::BlockPos;
use mc_protocol::components::EQUIPPABLE;
use mc_protocol::packets::play::Stat;

/// `net.minecraft.util.Prediction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prediction {
    Predicted,
    ServerOnly,
}

/// `Equippable.slot()` of `stack` (the `minecraft:equippable` component), if any.
pub(crate) fn equippable_slot(stack: &ItemStack) -> Option<EquipmentSlot> {
    stack
        .get(&EQUIPPABLE)
        .map(|e| EquipmentSlot::from_protocol(e.slot))
}

pub trait MenuPlayer {
    fn entity_id(&self) -> i32;
    /// `player.level().registryAccess()`.
    fn registries(&self) -> crate::registry::RegistryAccess;
    fn inventory(&self) -> &Inventory;
    fn inventory_mut(&mut self) -> &mut Inventory;
    /// `hasInfiniteMaterials()` (`abilities.instabuild`).
    fn has_infinite_materials(&self) -> bool;
    fn is_creative(&self) -> bool;
    fn is_spectator(&self) -> bool;
    /// `player instanceof ServerPlayer`.
    fn is_server_player(&self) -> bool {
        true
    }
    /// `isRemoved() && getRemovalReason() != CHANGED_DIMENSION`.
    fn is_removed_not_changing_dimension(&self) -> bool {
        false
    }
    /// `Player.mayBuild()` (`abilities.mayBuild`).
    fn may_build(&self) -> bool {
        true
    }
    /// `isAlive()`.
    fn is_alive(&self) -> bool {
        true
    }
    fn has_disconnected(&self) -> bool {
        false
    }
    /// `Player.canDropItems()`.
    fn can_drop_items(&self) -> bool {
        true
    }
    /// `Player.handleCreativeModeItemDrop` (no-op for plain players).
    fn handle_creative_mode_item_drop(&mut self, _stack: &ItemStack) {}
    /// `Player.drop(stack, thrownFromHand, prediction)`: spawns the item entity (swing, `createItemStackToDrop`).
    fn drop(&mut self, stack: ItemStack, thrown_from_hand: bool, prediction: Prediction);
    /// `LivingEntity.onEquipItem(slot, old, new)` (equip sound + game event).
    fn on_equip_item(&mut self, slot: EquipmentSlot, old: &ItemStack, new: &ItemStack);
    /// `LivingEntity.canUseSlot`.
    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }
    /// `LivingEntity.getEquipmentSlotForItem`.
    fn get_equipment_slot_for_item(&self, stack: &ItemStack) -> EquipmentSlot {
        match equippable_slot(stack) {
            Some(s) if self.can_use_slot(s) => s,
            _ => EquipmentSlot::MainHand,
        }
    }
    /// `LivingEntity.isEquippableInSlot` without `Equippable.canBeEquippedBy` (`allowed_entities`): the real player
    /// (`PlayerCtx`) overrides it with the entity type check.
    fn is_equippable_in_slot(&self, stack: &ItemStack, slot: EquipmentSlot) -> bool {
        match equippable_slot(stack) {
            None => slot == EquipmentSlot::MainHand && self.can_use_slot(EquipmentSlot::MainHand),
            Some(s) => slot == s && self.can_use_slot(s),
        }
    }
    /// `getContainerInteractionRange()`: the `BLOCK_INTERACTION_RANGE` attribute value (vanilla default 4.5).
    fn container_interaction_range(&self) -> f64 {
        4.5
    }
    /// `isWithinBlockInteractionRange(pos, buffer)`.
    fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool;
    /// `isWithinEntityInteractionRange(box, buffer)` (level-less test players are always in range).
    fn is_within_entity_interaction_range(
        &self,
        _target: &mc_core::phys::AABB,
        _buffer: f64,
    ) -> bool {
        true
    }
    /// Runs `f` with the real `ServerLevel`/`ServerPlayer` (advancement triggers, stats, xp, ...); no-op for test
    /// players. The player's own menus are detached while a menu runs.
    fn with_server_player(
        &mut self,
        _f: &mut dyn FnMut(&mut ServerLevel, &mut crate::world::entity::player::ServerPlayer),
    ) {
    }
    /// Read-only view of the player's level (`None` for level-less test players).
    fn level_ref(&self) -> Option<&ServerLevel> {
        None
    }
    /// `Player.getEnderChestInventory()` (`None` for test players).
    fn ender_chest(&self) -> Option<&super::PlayerEnderChestContainer> {
        None
    }
    fn ender_chest_mut(&mut self) -> Option<&mut super::PlayerEnderChestContainer> {
        None
    }
    /// Sends `ClientboundSetPlayerInventoryPacket(slot, stack)` (`Inventory.createInventoryUpdatePacket`).
    fn send_inventory_slot_update(&mut self, slot: usize, stack: ItemStack);
    /// `Entity.playSound(sound, volume, pitch)` from the player (no-op for test players).
    fn play_player_sound(&mut self, _sound: mc_data::SoundEvent, _volume: f32, _pitch: f32) {}
    /// `level.getRandom().nextFloat()` (the level's random).
    fn level_random_next_float(&mut self) -> f32 {
        0.0
    }
    /// The player's level (`None` for level-less test players).
    fn level(&mut self) -> Option<&mut ServerLevel> {
        None
    }
    /// `player.experienceLevel`.
    fn experience_level(&self) -> i32 {
        0
    }
    /// `Player.giveExperienceLevels(amount)`.
    fn give_experience_levels(&mut self, _amount: i32) {}
    /// `Player.getEnchantmentSeed()`.
    fn enchantment_seed(&self) -> i32 {
        0
    }
    /// `Player.onEnchantmentPerformed(stack, cost)` (xp levels, new seed).
    fn on_enchantment_performed(&mut self, _stack: &ItemStack, _enchantment_cost: i32) {}
    /// `player.awardStat(Stats.ENCHANT_ITEM)` + `CriteriaTriggers.ENCHANTED_ITEM.trigger(serverPlayer, stack, cost)`.
    fn on_item_enchanted(&mut self, _stack: &ItemStack, _enchantment_cost: i32) {}
    /// `player.getRandom().nextFloat()` (the entity's own random, not the level's).
    fn player_random_next_float(&mut self) -> f32 {
        0.0
    }
    /// `serverPlayer.getTextFilter().processStreamMessage(text)`.
    fn process_stream_message(&mut self, _text: &str) {}
    /// `getRecipeBook().contains(recipeId)` (test players know no recipe).
    fn recipe_book_contains(&self, _recipe_id: &mc_core::ident::Identifier) -> bool {
        false
    }
    /// `Entity.position()` (the feet).
    fn position(&self) -> mc_core::phys::Vec3 {
        mc_core::phys::Vec3::ZERO
    }
    /// `ServerPlayer.awardRecipes(recipes)`.
    fn award_recipes(&mut self, recipes: &[RecipeHolder]) {
        self.with_server_player(&mut |level, player| {
            player.award_recipes(level, recipes);
        });
    }
    /// `ServerPlayer.triggerRecipeCrafted(recipe, itemStacks)`.
    fn trigger_recipe_crafted(&mut self, recipe: &RecipeHolder, items: &[ItemStack]) {
        self.with_server_player(&mut |level, player| {
            player.trigger_recipe_crafted(level, recipe, items);
        });
    }
    /// `awardStat(stat, count)`.
    fn award_stat(&mut self, stat: Stat, count: i32) {
        self.with_server_player(&mut |level, player| {
            player.award_stat(&*level, stat, count);
        });
    }
}

/// `Inventory.placeItemBackInInventory(stack, shouldSendSetSlotPacket, prediction)`.
pub fn place_item_back_in_inventory(
    player: &mut dyn MenuPlayer,
    stack: &mut ItemStack,
    should_send_set_slot_packet: bool,
    prediction: Prediction,
) {
    while !stack.is_empty() {
        let mut slot = player.inventory().get_slot_with_remaining_space(stack);
        if slot == -1 {
            slot = player.inventory().get_free_slot();
        }
        if slot == -1 {
            // the dropped entity shares the stack object: the caller's stack keeps its contents
            player.drop(stack.clone(), false, prediction);
            break;
        }
        let space = stack.get_max_stack_size()
            - crate::world::container::Container::get_item(player.inventory(), slot as usize)
                .count();
        let mut part = stack.split(space);
        let infinite = player.has_infinite_materials();
        if player.inventory_mut().add_at(slot, &mut part, infinite)
            && should_send_set_slot_packet
            && player.is_server_player()
        {
            let updated =
                crate::world::container::Container::get_item(player.inventory(), slot as usize)
                    .copy();
            player.send_inventory_slot_update(slot as usize, updated);
        }
    }
}
