//! `DefaultGameModeCommands`.
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::arguments::GameModeArgument;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::util::game_modes::GameTypeExt;
use mc_game_core::{register_command, translatable};
use mc_world::level_storage::GameType;

register_command!("DefaultGameModeCommands", |ctx| {
    ctx.dispatcher.register(
        literal("defaultgamemode")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("gamemode", GameModeArgument::game_mode()).executes(|c| {
                    Ok(set_mode(
                        c.get_source(),
                        GameModeArgument::get_game_mode(c, "gamemode"),
                    ))
                }),
            ),
    );
});

fn set_mode(source: &CommandSourceStack, game_type: GameType) -> i32 {
    // `setDefaultGameType`, then `enforceGameTypeForPlayers(getForcedGameType())` (dedicated: `force-gamemode`).
    let count = source.with_server(|server| {
        server.world_data.lock().settings.game_type = game_type;
        let forced = server.get_forced_game_type();
        server.enforce_game_type_for_players(forced)
    });
    source.send_success(
        || {
            translatable!(
                "commands.defaultgamemode.success",
                game_type.get_long_display_name()
            )
        },
        true,
    );
    count
}
