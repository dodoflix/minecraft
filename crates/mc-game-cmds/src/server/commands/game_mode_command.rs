//! `GameModeCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, GameModeArgument};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::level::gamerules::GameRules;
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use mc_world::level_storage::GameType;

register_command!("GameModeCommand", |ctx| {
    ctx.dispatcher.register(
        literal("gamemode")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("gamemode", GameModeArgument::game_mode())
                    .executes(|c| {
                        let source = c.get_source();
                        let player = source.get_player_or_exception()?;
                        let target = EntityRef {
                            dimension: source.get_level().clone(),
                            id: player.id,
                        };
                        Ok(set_mode(
                            source,
                            &[target],
                            GameModeArgument::get_game_mode(c, "gamemode"),
                        ))
                    })
                    .then(argument("target", EntityArgument::players()).executes(|c| {
                        Ok(set_mode(
                            c.get_source(),
                            &EntityArgument::get_players(c, "target")?,
                            GameModeArgument::get_game_mode(c, "gamemode"),
                        ))
                    })),
            ),
    );
});

fn game_mode_name(game_type: GameType) -> &'static str {
    match game_type {
        GameType::Survival => "survival",
        GameType::Creative => "creative",
        GameType::Adventure => "adventure",
        GameType::Spectator => "spectator",
    }
}

fn log_game_mode_change(source: &CommandSourceStack, target: &EntityRef, new_type: GameType) {
    let mode = Component::translatable(format!("gameMode.{}", game_mode_name(new_type)), vec![]);
    if source.get_entity().is_some_and(|e| e.id == target.id) {
        source.send_success(
            || translatable!("commands.gamemode.success.self", mode.clone()),
            true,
        );
    } else {
        let (feedback, handle, display_name) = source.with_server(|server| {
            let handle = host::player_handle(server, target);
            let name = handle
                .as_ref()
                .map(|h| host::player_display_name(server, h));
            (
                server
                    .game_rules_ref()
                    .get(GameRules::SEND_COMMAND_FEEDBACK),
                handle,
                name,
            )
        });
        if feedback && let Some(handle) = &handle {
            handle.send_system_message(&translatable!("gameMode.changed", mode.clone()), false);
        }
        source.send_success(
            || {
                translatable!(
                    "commands.gamemode.success.other",
                    display_name.clone().unwrap_or_default(),
                    mode.clone()
                )
            },
            true,
        );
    }
}

fn set_mode(source: &CommandSourceStack, players: &[EntityRef], game_type: GameType) -> i32 {
    let mut count = 0;
    for player in players {
        if source.with_server(|server| host::set_game_mode(server, player, game_type)) {
            log_game_mode_change(source, player, game_type);
            count += 1;
        }
    }
    count
}
