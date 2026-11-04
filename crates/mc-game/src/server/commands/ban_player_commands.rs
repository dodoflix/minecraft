//! `BanPlayerCommands`.
use crate::commands::arguments::{GameProfileArgument, MessageArgument};
use crate::commands::message::simple_translatable;
use crate::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, literal};
use crate::server::players::{NameAndId, UserBanListEntry};
use mc_brigadier::ArgumentBuilder;
use crate::{register_command, translatable};
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_ALREADY_BANNED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.ban.failed"));

register_command!("BanPlayerCommands", |ctx| {
    ctx.dispatcher.register(
        literal("ban").requires(has_permission(LEVEL_ADMINS)).then(
            argument("targets", GameProfileArgument::game_profile())
                .executes(|c| ban_players(c.get_source(), GameProfileArgument::get_game_profiles(c, "targets")?, None))
                .then(argument("reason", MessageArgument::message()).executes(|c| {
                    ban_players(c.get_source(), GameProfileArgument::get_game_profiles(c, "targets")?, Some(MessageArgument::get_message(c, "reason")?))
                })),
        ),
    );
});

fn ban_players(source: &CommandSourceStack, players: Vec<NameAndId>, reason: Option<Component>) -> Result<i32, CommandSyntaxException> {
    let text_name = source.get_text_name();
    let mut count = 0;
    for player in players {
        let reason_string = reason.as_ref().map(|r| r.get_string());
        let entry = source.with_server(|server| {
            if server.player_list.bans.is_banned(&player) {
                return None;
            }
            let entry = UserBanListEntry::new(Some(player.clone()), None, Some(&text_name), None, reason_string.as_deref());
            server.player_list.bans.add(entry.clone());
            if let Some(online) = server.player_list.get_player(&player.id) {
                online.disconnect(&translatable!("multiplayer.disconnect.banned"));
            }
            Some(entry)
        });
        let Some(entry) = entry else { continue };
        count += 1;
        source.send_success(|| translatable!("commands.ban.success", Component::literal(player.name.clone()), entry.get_reason_message()), true);
    }
    if count == 0 { Err(ERROR_ALREADY_BANNED.create()) } else { Ok(count) }
}

