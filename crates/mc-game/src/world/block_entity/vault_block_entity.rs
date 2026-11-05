//! `VaultBlockEntity` (+ `Server.tick` / `tryInsertKey`).

use super::loot_roll::roll;
use super::vault::{VaultConfig, VaultServerData, VaultSharedData, VaultStateExt};
use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec, set_changed};
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{ItemStack, ItemStackExt};
use crate::world::level::BlockGetter;
use crate::world::level::storage::loot::loading::get_loot_table;
use crate::world::level::storage::loot::{LootContextParamSets, LootParams};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::VaultState;
use mc_data::{BlockEntityType, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::{ClientboundBlockEntityDataPacket, SoundSource};

/// `UNLOCKING_DELAY_TICKS`.
const UNLOCKING_DELAY_TICKS: i64 = 14;
/// `INSERT_FAIL_SOUND_BUFFER_TICKS`.
const INSERT_FAIL_SOUND_BUFFER_TICKS: i64 = 15;

pub struct VaultBlockEntity {
    base: BlockEntityData,
    pub server_data: VaultServerData,
    pub shared_data: VaultSharedData,
    pub config: VaultConfig,
}

impl VaultBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::VAULT, pos, state),
            server_data: VaultServerData::default(),
            shared_data: VaultSharedData::default(),
            config: super::vault::config::DEFAULT.clone(),
        }
    }
}

impl BlockEntity for VaultBlockEntity {
    block_entity_base!();

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Ok(config) = mc_dfu::encode_nbt(&VaultConfig::codec(), &self.config).into_result() {
            tag.put("config", config);
        }
        if let Ok(shared) = mc_dfu::encode_nbt(&VaultSharedData::codec(), &self.shared_data).into_result() {
            tag.put("shared_data", shared);
        }
        if let Ok(server) = mc_dfu::encode_nbt(&VaultServerData::codec(), &self.server_data).into_result() {
            tag.put("server_data", server);
        }
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        if let Some(server) = read_codec(tag, "server_data", &VaultServerData::codec()) {
            self.server_data.set(server);
        }
        self.config = read_codec(tag, "config", &VaultConfig::codec()).unwrap_or_else(|| super::vault::config::DEFAULT.clone());
        if let Some(shared) = read_codec(tag, "shared_data", &VaultSharedData::codec()) {
            self.shared_data.set(shared);
        }
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let state = level.get_block_state(pos);
        server_tick(level, pos, state, &self.config, &mut self.server_data, &mut self.shared_data);
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }

    /// `{shared_data}`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        if let Ok(shared) = mc_dfu::encode_nbt(&VaultSharedData::codec(), &self.shared_data).into_result() {
            tag.put("shared_data", shared);
        }
        tag
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static VAULT: BlockEntityRegistration = BlockEntityRegistration { ty: BlockEntityType::VAULT, create: |pos, state| Box::new(VaultBlockEntity::new(pos, state)) };

/// `VaultBlockEntity.Server.tick(...)`.
pub fn server_tick(level: &mut ServerLevel, pos: BlockPos, block_state: BlockState, config: &VaultConfig, server_data: &mut VaultServerData, shared_data: &mut VaultSharedData) {
    let current_state = block_state.get(props::VAULT_STATE);
    if should_cycle_display_item(level.game_time(), current_state) {
        cycle_display_item_from_loot_table(level, current_state, config, shared_data, pos);
    }
    let mut next_block_state = block_state;
    if level.game_time() >= server_data.state_updating_resumes_at {
        next_block_state = block_state.set(props::VAULT_STATE, current_state.tick_and_get_next(level, pos, config, server_data, shared_data));
        if block_state != next_block_state {
            set_vault_state(level, pos, block_state, next_block_state, config, shared_data);
        }
    }
    if server_data.is_dirty || shared_data.is_dirty {
        set_changed(level, pos, block_state);
        if shared_data.is_dirty {
            level.send_block_updated(pos, block_state, next_block_state, 2);
        }
        server_data.is_dirty = false;
        shared_data.is_dirty = false;
    }
}

/// `VaultBlockEntity.Server.tryInsertKey(...)`: `stack_to_insert` is the player's held stack (the caller writes it back).
pub fn try_insert_key(
    level: &mut ServerLevel,
    pos: BlockPos,
    block_state: BlockState,
    config: &VaultConfig,
    server_data: &mut VaultServerData,
    shared_data: &mut VaultSharedData,
    player: &mut ServerPlayer,
    stack_to_insert: &mut ItemStack,
) {
    let vault_state = block_state.get(props::VAULT_STATE);
    if !can_eject_reward(config, vault_state) {
        return;
    }
    if !is_valid_to_insert(config, stack_to_insert) {
        play_insert_fail_sound(level, server_data, pos, SoundEvent::BLOCK_VAULT_INSERT_ITEM_FAIL);
    } else if server_data.has_rewarded_player(&player.uuid()) {
        play_insert_fail_sound(level, server_data, pos, SoundEvent::BLOCK_VAULT_REJECT_REWARDED_PLAYER);
    } else {
        let items_to_eject = resolve_items_to_eject(level, config, pos, player, stack_to_insert);
        if !items_to_eject.is_empty() {
            player.award_stat_once(level, Stat::Used(stack_to_insert.item()));
            let infinite = player.has_infinite_materials();
            stack_to_insert.consume(config.key_item.count(), infinite);
            unlock(level, block_state, pos, config, server_data, shared_data, items_to_eject);
            server_data.add_to_rewarded_players(player.uuid());
            shared_data.update_connected_players_within_range(level, pos, server_data, config, config.deactivation_range);
        }
    }
}

/// `setVaultState(level, pos, currentBlockState, newBlockState, config, sharedData)`.
fn set_vault_state(level: &mut ServerLevel, pos: BlockPos, current: BlockState, new: BlockState, config: &VaultConfig, shared_data: &mut VaultSharedData) {
    let current_vault_state = current.get(props::VAULT_STATE);
    let new_vault_state = new.get(props::VAULT_STATE);
    crate::world::level::LevelAccessor::set_block_and_update(level, pos, new);
    current_vault_state.on_transition(level, pos, new_vault_state, config, shared_data, new.get(props::OMINOUS));
}

/// `cycleDisplayItemFromLootTable(level, vaultState, config, sharedData, pos)`.
pub fn cycle_display_item_from_loot_table(level: &mut ServerLevel, vault_state: VaultState, config: &VaultConfig, shared_data: &mut VaultSharedData, pos: BlockPos) {
    if !can_eject_reward(config, vault_state) {
        shared_data.set_display_item(ItemStack::empty());
    } else {
        let table = config.override_loot_table_to_display.clone().unwrap_or_else(|| config.loot_table.clone());
        let display_item = get_random_display_item_from_loot_table(level, pos, &table);
        shared_data.set_display_item(display_item);
    }
}

/// `getRandomDisplayItemFromLootTable(level, pos, lootTableId)`.
fn get_random_display_item_from_loot_table(level: &mut ServerLevel, pos: BlockPos, loot_table_id: &mc_core::ident::Identifier) -> ItemStack {
    let table = get_loot_table(&level.registries, loot_table_id);
    let results = {
        let params = LootParams::builder(level).origin(Vec3::at_center_of(&pos)).create(&LootContextParamSets::VAULT);
        roll(&params, &table, true)
    };
    if results.is_empty() {
        return ItemStack::empty();
    }
    // `Util.getRandom(results, level.getRandom())`
    let index = crate::world::level::LevelAccessor::random(level).next_int_bounded(results.len() as i32) as usize;
    results[index].clone()
}

/// `unlock(...)`.
fn unlock(level: &mut ServerLevel, block_state: BlockState, pos: BlockPos, config: &VaultConfig, server_data: &mut VaultServerData, shared_data: &mut VaultSharedData, items_to_eject: Vec<ItemStack>) {
    server_data.set_items_to_eject(items_to_eject);
    shared_data.set_display_item(server_data.get_next_item_to_eject());
    server_data.pause_state_updating_until(level.game_time() + UNLOCKING_DELAY_TICKS);
    set_vault_state(level, pos, block_state, block_state.set(props::VAULT_STATE, VaultState::Unlocking), config, shared_data);
}

/// `resolveItemsToEject(level, config, pos, player, insertedStack)`.
fn resolve_items_to_eject(level: &mut ServerLevel, config: &VaultConfig, pos: BlockPos, player: &ServerPlayer, inserted_stack: &ItemStack) -> Vec<ItemStack> {
    let table = get_loot_table(&level.registries, &config.loot_table);
    let luck = player.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::LUCK) as f32);
    let items = {
        let params = LootParams::builder(level)
            .origin(Vec3::at_center_of(&pos))
            .luck(luck)
            .this_entity(player)
            .tool(inserted_stack)
            .create(&LootContextParamSets::VAULT);
        roll(&params, &table, false)
    };
    items
}

/// `canEjectReward(config, vaultState)`.
fn can_eject_reward(config: &VaultConfig, vault_state: VaultState) -> bool {
    !config.key_item.is_empty() && vault_state != VaultState::Inactive
}

/// `isValidToInsert(config, stackToInsert)`.
fn is_valid_to_insert(config: &VaultConfig, stack_to_insert: &ItemStack) -> bool {
    ItemStack::is_same_item_same_components(stack_to_insert, &config.key_item) && stack_to_insert.count() >= config.key_item.count()
}

/// `shouldCycleDisplayItem(gameTime, vaultState)`.
fn should_cycle_display_item(game_time: i64, vault_state: VaultState) -> bool {
    game_time % 20 == 0 && vault_state == VaultState::Active
}

/// `playInsertFailSound(level, serverData, pos, sound)`.
fn play_insert_fail_sound(level: &mut ServerLevel, server_data: &mut VaultServerData, pos: BlockPos, sound: SoundEvent) {
    if level.game_time() >= server_data.last_insert_fail_timestamp + INSERT_FAIL_SOUND_BUFFER_TICKS {
        level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
        server_data.last_insert_fail_timestamp = level.game_time();
    }
}
