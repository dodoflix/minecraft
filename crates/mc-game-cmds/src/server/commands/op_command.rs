//! `OpCommand`.
use mc_game_core::commands::arguments::GameProfileArgument;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, host, literal};
use mc_game_core::server::players::NameAndId;
use mc_brigadier::ArgumentBuilder;
use mc_game_core::{register_command, translatable};
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use std::sync::LazyLock;

static ERROR_ALREADY_OP: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.op.failed"));

register_command!("OpCommand", |ctx| {
    ctx.dispatcher.register(literal("op").requires(has_permission(LEVEL_ADMINS)).then(
        argument("targets", GameProfileArgument::game_profile())
            .suggests(|c, p| {
                let names = c.get_source().with_server(|server| {
                    let list = &server.player_list;
                    list.get_players().iter().filter(|player| !list.is_op(&player.name_and_id())).map(|p| p.name().to_owned()).collect::<Vec<_>>()
                });
                Ok(suggest(names, p))
            })
            .executes(|c| op_players(c.get_source(), GameProfileArgument::get_game_profiles(c, "targets")?)),
    ));
});

fn op_players(source: &CommandSourceStack, players: Vec<NameAndId>) -> Result<i32, CommandSyntaxException> {
    let mut count = 0;
    for player in players {
        if source.with_server(|server| {
            let was_op = server.player_list.is_op(&player);
            if !was_op {
                host::op(server, player.clone());
            }
            !was_op
        }) {
            count += 1;
            source.send_success(|| translatable!("commands.op.success", player.name.clone()), true);
        }
    }
    if count == 0 { Err(ERROR_ALREADY_OP.create()) } else { Ok(count) }
}
