//! `KickCommand`.
use crate::commands::arguments::{EntityArgument, MessageArgument};
use crate::commands::message::simple_translatable;
use crate::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, host, literal};
use crate::commands::arguments::selector::EntityRef;
use mc_brigadier::ArgumentBuilder;
use crate::{register_command, translatable};
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_KICKING_OWNER: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.kick.owner.failed"));

register_command!("KickCommand", |ctx| {
    ctx.dispatcher.register(
        literal("kick").requires(has_permission(LEVEL_ADMINS)).then(
            argument("targets", EntityArgument::players())
                .executes(|c| kick_players(c.get_source(), EntityArgument::get_players(c, "targets")?, translatable!("multiplayer.disconnect.kicked")))
                .then(argument("reason", MessageArgument::message()).executes(|c| {
                    kick_players(c.get_source(), EntityArgument::get_players(c, "targets")?, MessageArgument::get_message(c, "reason")?)
                })),
        ),
    );
});

/// `isPublished()` is always true on a dedicated server and nobody is the singleplayer owner.
fn kick_players(source: &CommandSourceStack, players: Vec<EntityRef>, reason: Component) -> Result<i32, CommandSyntaxException> {
    let mut count = 0;
    for player in players {
        let Some(display_name) = source.with_server(|server| host::kick_player(server, &player, &reason)) else { continue };
        source.send_success(|| translatable!("commands.kick.success", display_name, reason.clone()), true);
        count += 1;
    }
    if count == 0 { Err(ERROR_KICKING_OWNER.create()) } else { Ok(count) }
}
