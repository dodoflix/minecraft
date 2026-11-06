//! `TellRawCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::arguments::{ComponentArgument, EntityArgument};
use mc_game_core::commands::{LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::register_command;

register_command!("TellRawCommand", |ctx| {
    ctx.dispatcher.register(
        literal("tellraw")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(argument("targets", EntityArgument::players()).then(
                argument("message", ComponentArgument::text_component()).executes(|c| {
                    let mut result = 0;
                    for player in EntityArgument::get_players(c, "targets")? {
                        let message = ComponentArgument::get_resolved_component(
                            c,
                            "message",
                            player.clone(),
                        )?;
                        c.get_source().with_server(|server| {
                            if let Some(handle) = host::player_handle(server, &player) {
                                handle.send_system_message(&message, false);
                            }
                        });
                        result += 1;
                    }
                    Ok(result)
                }),
            )),
    );
});
