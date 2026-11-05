//! `PardonIpCommand`.
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, literal};
use mc_brigadier::ArgumentBuilder;
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use std::sync::LazyLock;

static ERROR_INVALID: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.pardonip.invalid"));
static ERROR_NOT_BANNED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.pardonip.failed"));

register_command!("PardonIpCommand", |ctx| {
    ctx.dispatcher.register(literal("pardon-ip").requires(has_permission(LEVEL_ADMINS)).then(
        argument("target", StringArgumentType::word())
            .suggests(|c, p| Ok(suggest(c.get_source().with_server(|server| server.player_list.ip_bans.get_user_list()), p)))
            .executes(|c| unban(c.get_source(), StringArgumentType::get_string(c, "target"))),
    ));
});

fn unban(source: &CommandSourceStack, ip: &str) -> Result<i32, CommandSyntaxException> {
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(ERROR_INVALID.create());
    }
    let removed = source.with_server(|server| server.player_list.ip_bans.is_banned(ip) && server.player_list.ip_bans.remove(ip));
    if !removed {
        return Err(ERROR_NOT_BANNED.create());
    }
    source.send_success(|| translatable!("commands.pardonip.success", ip.to_owned()), true);
    Ok(1)
}
