//! `MinecraftServer.reloadResources(packsToEnable)`: swap the data pack resources of a running server.

use super::minecraft_server::ReloadableResources;
use super::packs::get_selected_packs;
use super::reloadable_resources::{ReloadError, reload_resources};
use super::MinecraftServer;
use mc_world::level_storage::WorldDataConfiguration;

impl MinecraftServer {
    /// Reloads the registries/tags/functions/commands against `packs_to_enable` and, on success, makes them the server's.
    /// On `Err` nothing changed (the caller logs `Failed to execute reload` and tells the source). Runs synchronously: vanilla's
    /// `managedBlock` over the async reload ends in the same state before the command returns.
    pub fn reload_resources(&mut self, packs_to_enable: &[String]) -> Result<(), ReloadError> {
        let enabled_features = self.world_data.lock().settings.data_configuration.enabled_features;
        let permissions = self.get_function_compilation_permissions().into();
        let reloaded = reload_resources(&self.pack_repository, packs_to_enable, &self.registries, enabled_features, &permissions)?;
        self.resources.resource_manager.close();
        self.resources = ReloadableResources { resource_manager: reloaded.resource_manager, managers: reloaded.managers };
        self.pack_repository.set_selected(packs_to_enable);
        let new_config = WorldDataConfiguration { data_packs: Some(get_selected_packs(&self.pack_repository, true)), enabled_features };
        self.world_data.lock().settings.data_configuration = new_config;
        self.registries = self.resources.managers.registry_layers().clone();
        self.commands = self.resources.managers.commands().clone();
        crate::server::players::player_ops::save_all(self);
        crate::server::players::player_ops::reload_resources(self);
        let library = self.resources.managers.listener::<super::server_function_library::ServerFunctionLibrary>().map(|l| l.data().clone()).unwrap_or_default();
        self.functions.replace_library(library);
        Ok(())
    }
}
