//! `DefaultGameModeCommands`.
use crate::commands::arguments::GameModeArgument;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use crate::util::game_modes::GameTypeExt;
use crate::{register_command, translatable};
use mc_brigadier::ArgumentBuilder;
use mc_world::level_storage::GameType;

register_command!("DefaultGameModeCommands", |ctx| {
    ctx.dispatcher.register(
        literal("defaultgamemode")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(argument("gamemode", GameModeArgument::game_mode()).executes(|c| Ok(set_mode(c.get_source(), GameModeArgument::get_game_mode(c, "gamemode"))))),
    );
});

fn set_mode(source: &CommandSourceStack, game_type: GameType) -> i32 {
    // `setDefaultGameType`, then `enforceGameTypeForPlayers(getForcedGameType())` (dedicated: `force-gamemode`).
    let count = source.with_server(|server| {
        server.world_data.lock().settings.game_type = game_type;
        let Some(forced) = server.get_forced_game_type() else { return 0 };
        let mut count = 0;
        for player in host::server_players(server) {
            if host::set_game_mode(server, &player, forced) {
                count += 1;
            }
        }
        count
    });
    source.send_success(|| translatable!("commands.defaultgamemode.success", game_type.get_long_display_name()), true);
    count
}
