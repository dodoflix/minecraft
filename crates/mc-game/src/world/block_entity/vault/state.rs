//! `VaultState`: light levels and the state machine step.

use super::config::VaultConfig;
use super::server_data::VaultServerData;
use super::shared_data::VaultSharedData;
use crate::server::level::ServerLevel;
use crate::world::block_entity::loot_roll::{reward_eject_position, spawn_item};
use crate::world::block_entity::vault_block_entity;
use crate::world::item::ItemStack;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::SoundEvent;
use mc_data::enums::VaultState;
use mc_protocol::packets::play::level::SoundSource;

pub trait VaultStateExt: Copy {
    /// `lightLevel()`.
    fn light_level(self) -> i32;
    /// `tickAndGetNext(level, pos, config, serverData, sharedData)`.
    fn tick_and_get_next(self, level: &mut ServerLevel, pos: BlockPos, config: &VaultConfig, server_data: &mut VaultServerData, shared_data: &mut VaultSharedData) -> VaultState;
    /// `onTransition(level, pos, to, config, sharedData, isOminous)`.
    fn on_transition(self, level: &mut ServerLevel, pos: BlockPos, to: VaultState, config: &VaultConfig, shared_data: &mut VaultSharedData, is_ominous: bool);
}

impl VaultStateExt for VaultState {
    fn light_level(self) -> i32 {
        if self == VaultState::Inactive { 6 } else { 12 }
    }

    fn tick_and_get_next(self, level: &mut ServerLevel, pos: BlockPos, config: &VaultConfig, server_data: &mut VaultServerData, shared_data: &mut VaultSharedData) -> VaultState {
        match self {
            VaultState::Inactive => update_state_for_connected_players(level, pos, config, server_data, shared_data, config.activation_range),
            VaultState::Active => update_state_for_connected_players(level, pos, config, server_data, shared_data, config.deactivation_range),
            VaultState::Unlocking => {
                server_data.pause_state_updating_until(level.game_time() + 20);
                VaultState::Ejecting
            }
            VaultState::Ejecting => {
                if server_data.items_to_eject.is_empty() {
                    server_data.mark_ejection_finished();
                    update_state_for_connected_players(level, pos, config, server_data, shared_data, config.deactivation_range)
                } else {
                    let ejection_sound_progress = server_data.ejection_progress();
                    let item = server_data.pop_next_item_to_eject();
                    eject_result_item(level, pos, item, ejection_sound_progress);
                    shared_data.set_display_item(server_data.get_next_item_to_eject());
                    server_data.pause_state_updating_until(level.game_time() + 20);
                    VaultState::Ejecting
                }
            }
        }
    }

    fn on_transition(self, level: &mut ServerLevel, pos: BlockPos, to: VaultState, config: &VaultConfig, shared_data: &mut VaultSharedData, is_ominous: bool) {
        // onExit
        if self == VaultState::Ejecting {
            level.play_sound(None, pos, SoundEvent::BLOCK_VAULT_CLOSE_SHUTTER, SoundSource::Blocks, 1.0, 1.0);
        }
        // onEnter
        match to {
            VaultState::Inactive => {
                shared_data.set_display_item(ItemStack::empty());
                level.level_event(None, 3016, pos, i32::from(is_ominous));
            }
            VaultState::Active => {
                if !shared_data.has_display_item() {
                    vault_block_entity::cycle_display_item_from_loot_table(level, to, config, shared_data, pos);
                }
                level.level_event(None, 3015, pos, i32::from(is_ominous));
            }
            VaultState::Unlocking => level.play_sound(None, pos, SoundEvent::BLOCK_VAULT_INSERT_ITEM, SoundSource::Blocks, 1.0, 1.0),
            VaultState::Ejecting => level.play_sound(None, pos, SoundEvent::BLOCK_VAULT_OPEN_SHUTTER, SoundSource::Blocks, 1.0, 1.0),
        }
    }
}

/// `updateStateForConnectedPlayers(...)`.
fn update_state_for_connected_players(level: &mut ServerLevel, pos: BlockPos, config: &VaultConfig, server_data: &mut VaultServerData, shared_data: &mut VaultSharedData, activation_range: f64) -> VaultState {
    shared_data.update_connected_players_within_range(level, pos, server_data, config, activation_range);
    server_data.pause_state_updating_until(level.game_time() + 20);
    if shared_data.has_connected_players() { VaultState::Active } else { VaultState::Inactive }
}

/// `ejectResultItem(level, pos, item, ejectionSoundProgress)`.
fn eject_result_item(level: &mut ServerLevel, pos: BlockPos, item: ItemStack, ejection_sound_progress: f32) {
    spawn_item(level, item, 2, Direction::Up, reward_eject_position(pos));
    level.level_event(None, 3017, pos, 0);
    level.play_sound(None, pos, SoundEvent::BLOCK_VAULT_EJECT_ITEM, SoundSource::Blocks, 1.0, 0.8 + 0.4 * ejection_sound_progress);
}
