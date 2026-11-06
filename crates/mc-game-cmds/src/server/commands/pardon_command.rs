//! `PardonCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_game_core::commands::arguments::GameProfileArgument;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, literal};
use mc_game_core::server::players::NameAndId;
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_NOT_BANNED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.pardon.failed"));

register_command!("PardonCommand", |ctx| {
    ctx.dispatcher.register(
        literal("pardon")
            .requires(has_permission(LEVEL_ADMINS))
            .then(
                argument("targets", GameProfileArgument::game_profile())
                    .suggests(|c, p| {
                        Ok(suggest(
                            c.get_source()
                                .with_server(|server| server.player_list.bans.get_user_list()),
                            p,
                        ))
                    })
                    .executes(|c| {
                        pardon_players(
                            c.get_source(),
                            GameProfileArgument::get_game_profiles(c, "targets")?,
                        )
                    }),
            ),
    );
});

fn pardon_players(
    source: &CommandSourceStack,
    players: Vec<NameAndId>,
) -> Result<i32, CommandSyntaxException> {
    let mut count = 0;
    for player in players {
        let removed = source.with_server(|server| {
            server.player_list.bans.is_banned(&player) && server.player_list.bans.remove(&player)
        });
        if removed {
            count += 1;
            source.send_success(
                || {
                    translatable!(
                        "commands.pardon.success",
                        Component::literal(player.name.clone())
                    )
                },
                true,
            );
        }
    }
    if count == 0 {
        Err(ERROR_NOT_BANNED.create())
    } else {
        Ok(count)
    }
}
