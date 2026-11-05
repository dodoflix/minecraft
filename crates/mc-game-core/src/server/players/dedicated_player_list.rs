//! `net.minecraft.server.dedicated.DedicatedPlayerList`: the constructor's list loading (the `isWhiteListed` /
//! `canBypassPlayerLimit` overrides live in `PlayerList` itself, the dedicated server being the only one).

use super::player_list::{PlayerList, PlayerListSettings};
use crate::server::notifications::NotificationService;
use mc_world::level_storage::PlayerDataStorage;
use std::path::PathBuf;
use std::sync::Arc;

pub struct DedicatedPlayerList;

impl DedicatedPlayerList {
    /// `new DedicatedPlayerList(server, registries, playerDataStorage)`: `view_distance`/`simulation_distance` are
    /// `server.viewDistance()`/`simulationDistance()`. Levels do not exist yet at this point of `initServer`, so the
    /// vanilla `getAllLevels()` loops inside `setViewDistance` have nothing to update; the levels read the values when
    /// they are created.
    pub fn new(
        settings: PlayerListSettings,
        player_io: PlayerDataStorage,
        stats_dir: PathBuf,
        advancements_dir: PathBuf,
        notifications: Arc<dyn NotificationService>,
        view_distance: i32,
        simulation_distance: i32,
    ) -> PlayerList {
        let mut list = PlayerList::new(settings, player_io, stats_dir, advancements_dir, notifications);
        list.set_view_distance_value(view_distance);
        list.set_simulation_distance_value(simulation_distance);
        list.load_user_ban_list();
        list.save_user_ban_list();
        list.load_ip_ban_list();
        list.save_ip_ban_list();
        list.load_ops();
        list.load_white_list();
        list.save_ops();
        if !list.whitelist.get_file().exists() {
            list.save_white_list();
        }
        list
    }
}

impl PlayerList {
    /// The field assignment of `setViewDistance`.
    pub fn set_view_distance_value(&mut self, view_distance: i32) {
        self.view_distance = view_distance;
    }

    /// The field assignment of `setSimulationDistance`.
    pub fn set_simulation_distance_value(&mut self, simulation_distance: i32) {
        self.simulation_distance = simulation_distance;
    }

    fn save_ip_ban_list(&self) {
        if let Err(e) = self.ip_bans.save() {
            log::warn!("Failed to save ip banlist: {e}");
        }
    }

    fn save_user_ban_list(&self) {
        if let Err(e) = self.bans.save() {
            log::warn!("Failed to save user banlist: {e}");
        }
    }

    fn load_ip_ban_list(&mut self) {
        if let Err(e) = self.ip_bans.load() {
            log::warn!("Failed to load ip banlist: {e}");
        }
    }

    fn load_user_ban_list(&mut self) {
        if let Err(e) = self.bans.load() {
            log::warn!("Failed to load user banlist: {e}");
        }
    }

    fn load_ops(&mut self) {
        if let Err(e) = self.ops.load() {
            log::warn!("Failed to load operators list: {e}");
        }
    }

    fn save_ops(&self) {
        if let Err(e) = self.ops.save() {
            log::warn!("Failed to save operators list: {e}");
        }
    }

    fn load_white_list(&mut self) {
        if let Err(e) = self.whitelist.load() {
            log::warn!("Failed to load white-list: {e}");
        }
    }

    fn save_white_list(&self) {
        if let Err(e) = self.whitelist.save() {
            log::warn!("Failed to save white-list: {e}");
        }
    }

    /// `reloadWhiteList()` (`DedicatedPlayerList` reloads the file).
    pub fn reload_white_list(&mut self) {
        self.load_white_list();
    }
}
