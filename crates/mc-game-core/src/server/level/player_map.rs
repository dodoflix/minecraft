//! `PlayerMap`: the players of a chunk map with an "ignored" flag (spectators that don't generate chunks).
use crate::world::entity::EntityId;
use indexmap::IndexMap;

#[derive(Default)]
pub struct PlayerMap {
    players: IndexMap<EntityId, bool>,
}

impl PlayerMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_all_players(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.players.keys().copied()
    }

    pub fn add_player(&mut self, player: EntityId, ignored: bool) {
        self.players.insert(player, ignored);
    }

    pub fn remove_player(&mut self, player: EntityId) {
        self.players.shift_remove(&player);
    }

    pub fn ignore_player(&mut self, player: EntityId) {
        if let Some(v) = self.players.get_mut(&player) {
            *v = true;
        }
    }

    pub fn un_ignore_player(&mut self, player: EntityId) {
        if let Some(v) = self.players.get_mut(&player) {
            *v = false;
        }
    }

    pub fn ignored_or_unknown(&self, player: EntityId) -> bool {
        self.players.get(&player).copied().unwrap_or(true)
    }

    pub fn ignored(&self, player: EntityId) -> bool {
        self.players.get(&player).copied().unwrap_or(false)
    }
}
