//! `ReloadCommand`: `/reload` (and `reloadPacks`, which `/datapack` shares).
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, has_permission, literal};
use mc_game_core::{register_command, translatable};

/// `ReloadCommand.reloadPacks(selectedPacks, source)`.
pub fn reload_packs(selected_packs: &[String], source: &CommandSourceStack) {
    if let Err(e) = source.with_server(|server| server.reload_resources(selected_packs)) {
        log::warn!(
            "Failed to execute reload\n{}",
            mc_game_core::server::packs::throwable(
                "java.util.concurrent.CompletionException",
                &e.to_string()
            )
        );
        source.send_failure(translatable!("commands.reload.failure"));
    }
}

/// `ReloadCommand.discoverNewPacks`: the current selection plus every available pack the world did not disable.
fn discover_new_packs(source: &CommandSourceStack) -> Vec<String> {
    source.with_server(|server| {
        server.pack_repository.reload();
        let mut selected = server.pack_repository.selected_ids();
        let disabled = server
            .world_data
            .lock()
            .settings
            .data_configuration
            .data_packs
            .as_ref()
            .map(|c| c.disabled.clone())
            .unwrap_or_default();
        for pack in server.pack_repository.available_ids() {
            if !disabled.iter().any(|d| d == pack) && !selected.iter().any(|s| s == pack) {
                selected.push(pack.to_owned());
            }
        }
        selected
    })
}

register_command!("ReloadCommand", |ctx| {
    ctx.dispatcher.register(
        literal("reload")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| {
                let source = c.get_source();
                let new_selected_packs = discover_new_packs(source);
                source.send_success(|| translatable!("commands.reload.success"), true);
                reload_packs(&new_selected_packs, source);
                Ok(0)
            }),
    );
});
