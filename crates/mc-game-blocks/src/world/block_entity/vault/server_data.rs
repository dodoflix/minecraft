//! `VaultServerData`: who got a reward, the items still to eject and the state timer.

use mc_game_core::world::item::ItemStack;
use mc_dfu::{Codec, INT, LONG, MapCodec, record};
use mc_protocol::components::uuid_codec;
use std::sync::LazyLock;
use uuid::Uuid;

/// `MAX_REWARD_PLAYERS`.
const MAX_REWARD_PLAYERS: usize = 128;

#[derive(Clone, Debug, Default)]
pub struct VaultServerData {
    /// `rewardedPlayers`: an `ObjectLinkedOpenHashSet` (insertion order).
    pub rewarded_players: Vec<Uuid>,
    pub state_updating_resumes_at: i64,
    pub items_to_eject: Vec<ItemStack>,
    pub last_insert_fail_timestamp: i64,
    pub total_ejections_needed: i32,
    pub is_dirty: bool,
}

/// The fields of `VaultServerData.CODEC`.
struct Saved {
    rewarded_players: Vec<Uuid>,
    state_updating_resumes_at: i64,
    items_to_eject: Vec<ItemStack>,
    total_ejections_needed: i32,
}

impl VaultServerData {
    /// `VaultServerData.CODEC`.
    pub fn codec() -> Codec<VaultServerData> {
        static C: LazyLock<Codec<VaultServerData>> = LazyLock::new(|| {
            let map: MapCodec<Saved> = record!(Saved {
                rewarded_players: uuid_codec().list_of().lenient_optional_field_of_default("rewarded_players", Vec::new()),
                state_updating_resumes_at: LONG.lenient_optional_field_of_default("state_updating_resumes_at", 0),
                items_to_eject: ItemStack::codec().list_of().lenient_optional_field_of_default("items_to_eject", Vec::new()),
                total_ejections_needed: INT.lenient_optional_field_of_default("total_ejections_needed", 0),
            });
            map.codec().xmap(
                |s: Saved| {
                    let mut data = VaultServerData {
                        state_updating_resumes_at: s.state_updating_resumes_at,
                        items_to_eject: s.items_to_eject,
                        total_ejections_needed: s.total_ejections_needed,
                        ..Default::default()
                    };
                    for player in s.rewarded_players {
                        if !data.rewarded_players.contains(&player) {
                            data.rewarded_players.push(player);
                        }
                    }
                    data
                },
                |d: &VaultServerData| Saved {
                    rewarded_players: d.rewarded_players.clone(),
                    state_updating_resumes_at: d.state_updating_resumes_at,
                    items_to_eject: d.items_to_eject.clone(),
                    total_ejections_needed: d.total_ejections_needed,
                },
            )
        });
        C.clone()
    }

    /// `hasRewardedPlayer(player)`.
    pub fn has_rewarded_player(&self, player: &Uuid) -> bool {
        self.rewarded_players.contains(player)
    }

    /// `addToRewardedPlayers(player)`: the oldest entry is dropped beyond 128.
    pub fn add_to_rewarded_players(&mut self, player: Uuid) {
        if !self.rewarded_players.contains(&player) {
            self.rewarded_players.push(player);
        }
        if self.rewarded_players.len() > MAX_REWARD_PLAYERS {
            self.rewarded_players.remove(0);
        }
        self.mark_changed();
    }

    /// `pauseStateUpdatingUntil(resumesAt)`.
    pub fn pause_state_updating_until(&mut self, resumes_at: i64) {
        self.state_updating_resumes_at = resumes_at;
        self.mark_changed();
    }

    /// `markEjectionFinished()`.
    pub fn mark_ejection_finished(&mut self) {
        self.total_ejections_needed = 0;
        self.mark_changed();
    }

    /// `setItemsToEject(items)`.
    pub fn set_items_to_eject(&mut self, items: Vec<ItemStack>) {
        self.total_ejections_needed = items.len() as i32;
        self.items_to_eject = items;
        self.mark_changed();
    }

    /// `getNextItemToEject()`: the last element.
    pub fn get_next_item_to_eject(&self) -> ItemStack {
        self.items_to_eject.last().cloned().unwrap_or_else(ItemStack::empty)
    }

    /// `popNextItemToEject()`.
    pub fn pop_next_item_to_eject(&mut self) -> ItemStack {
        match self.items_to_eject.pop() {
            Some(item) => {
                self.mark_changed();
                item
            }
            None => ItemStack::empty(),
        }
    }

    /// `set(from)`: the decoded saved data replaces timer, items and rewarded players (not the ejection counter).
    pub fn set(&mut self, from: VaultServerData) {
        self.state_updating_resumes_at = from.state_updating_resumes_at;
        self.items_to_eject = from.items_to_eject;
        self.rewarded_players = from.rewarded_players;
    }

    fn mark_changed(&mut self) {
        self.is_dirty = true;
    }

    /// `ejectionProgress()`.
    pub fn ejection_progress(&self) -> f32 {
        if self.total_ejections_needed == 1 {
            1.0
        } else {
            // `1.0F - Mth.inverseLerp((float) size, 1.0F, (float) total)`
            1.0 - ((self.items_to_eject.len() as f32 - 1.0) / (self.total_ejections_needed as f32 - 1.0))
        }
    }
}
