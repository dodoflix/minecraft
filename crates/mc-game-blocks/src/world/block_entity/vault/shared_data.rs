//! `VaultSharedData`: the part of the vault state that is sent to clients.

use super::config::{self, VaultConfig};
use super::server_data::VaultServerData;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::util::java_uuid_hash_set::UuidHashSet;
use crate::world::block_entity::trialspawner::PlayerDetector;
use mc_game_core::world::item::ItemStack;
use mc_core::pos::BlockPos;
use mc_dfu::{Codec, DOUBLE, MapCodec, record};
use mc_protocol::components::uuid_codec;
use std::sync::LazyLock;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct VaultSharedData {
    pub display_item: ItemStack,
    /// `connectedPlayers` in the iteration order of the Java set (linked for saved data, a `HashSet` after an update).
    pub connected_players: Vec<Uuid>,
    pub connected_particles_range: f64,
    pub is_dirty: bool,
}

impl Default for VaultSharedData {
    fn default() -> Self {
        Self { display_item: ItemStack::empty(), connected_players: Vec::new(), connected_particles_range: config::DEFAULT.deactivation_range, is_dirty: false }
    }
}

struct Saved {
    display_item: ItemStack,
    connected_players: Vec<Uuid>,
    connected_particles_range: f64,
}

impl VaultSharedData {
    /// `VaultSharedData.CODEC`.
    pub fn codec() -> Codec<VaultSharedData> {
        static C: LazyLock<Codec<VaultSharedData>> = LazyLock::new(|| {
            let map: MapCodec<Saved> = record!(Saved {
                display_item: ItemStack::codec().lenient_optional_field_of("display_item").xmap(
                    |item: Option<ItemStack>| item.unwrap_or_else(ItemStack::empty),
                    |item: &ItemStack| if item.is_empty() { None } else { Some(item.clone()) },
                ),
                connected_players: uuid_codec().list_of().lenient_optional_field_of_default("connected_players", Vec::new()),
                connected_particles_range: DOUBLE.lenient_optional_field_of_default("connected_particles_range", config::DEFAULT.deactivation_range),
            });
            map.codec().xmap(
                |s: Saved| {
                    let mut connected_players = Vec::new();
                    for player in s.connected_players {
                        if !connected_players.contains(&player) {
                            connected_players.push(player);
                        }
                    }
                    VaultSharedData { display_item: s.display_item, connected_players, connected_particles_range: s.connected_particles_range, is_dirty: false }
                },
                |d: &VaultSharedData| Saved {
                    display_item: d.display_item.clone(),
                    connected_players: d.connected_players.clone(),
                    connected_particles_range: d.connected_particles_range,
                },
            )
        });
        C.clone()
    }

    /// `hasDisplayItem()`.
    pub fn has_display_item(&self) -> bool {
        !self.display_item.is_empty()
    }

    /// `setDisplayItem(stack)`.
    pub fn set_display_item(&mut self, stack: ItemStack) {
        if !ItemStack::matches(&self.display_item, &stack) {
            self.display_item = stack;
            self.is_dirty = true;
        }
    }

    /// `hasConnectedPlayers()`.
    pub fn has_connected_players(&self) -> bool {
        !self.connected_players.is_empty()
    }

    /// `updateConnectedPlayersWithinRange(level, pos, serverData, config, limit)`.
    pub fn update_connected_players_within_range(&mut self, level: &ServerLevel, pos: BlockPos, server_data: &VaultServerData, _config: &VaultConfig, limit: f64) {
        let mut current = UuidHashSet::new();
        for uuid in PlayerDetector::IncludingCreativePlayers.detect(level, pos, limit, false) {
            if !server_data.rewarded_players.contains(&uuid) {
                current.insert(uuid);
            }
        }
        let same = current.len() == self.connected_players.len() && self.connected_players.iter().all(|u| current.contains(u));
        if !same {
            self.connected_players = current.iter().copied().collect();
            self.is_dirty = true;
        }
    }

    /// `set(from)`.
    pub fn set(&mut self, from: VaultSharedData) {
        self.display_item = from.display_item;
        self.connected_players = from.connected_players;
        self.connected_particles_range = from.connected_particles_range;
    }
}
