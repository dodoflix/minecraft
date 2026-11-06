//! `DeOpCommands`.
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_game_core::commands::arguments::GameProfileArgument;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_ADMINS, argument, has_permission, host, literal,
};
use mc_game_core::server::players::NameAndId;
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

static ERROR_NOT_OP: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.deop.failed"));

register_command!("DeOpCommands", |ctx| {
    ctx.dispatcher.register(
        literal("deop").requires(has_permission(LEVEL_ADMINS)).then(
            argument("targets", GameProfileArgument::game_profile())
                .suggests(|c, p| {
                    Ok(suggest(
                        c.get_source()
                            .with_server(|server| server.player_list.get_op_names()),
                        p,
                    ))
                })
                .executes(|c| {
                    deop_players(
                        c.get_source(),
                        GameProfileArgument::get_game_profiles(c, "targets")?,
                    )
                }),
        ),
    );
});

fn deop_players(
    source: &CommandSourceStack,
    players: Vec<NameAndId>,
) -> Result<i32, CommandSyntaxException> {
    let mut count = 0;
    for player in players {
        if source.with_server(|server| {
            let is_op = server.player_list.is_op(&player);
            if is_op {
                host::deop(server, player.clone());
            }
            is_op
        }) {
            count += 1;
            source.send_success(
                || translatable!("commands.deop.success", player.name.clone()),
                true,
            );
        }
    }
    if count == 0 {
        Err(ERROR_NOT_OP.create())
    } else {
        source.with_server(host::kick_unlisted_players);
        Ok(count)
    }
}
