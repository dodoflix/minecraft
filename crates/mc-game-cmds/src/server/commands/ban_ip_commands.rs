//! `BanIpCommands`.
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_game_core::commands::arguments::MessageArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::selector::EntitySelector;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_ADMINS, argument, has_permission, host, literal,
};
use mc_game_core::server::players::IpBanListEntry;
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_INVALID_IP: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.banip.invalid"));
static ERROR_ALREADY_BANNED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.banip.failed"));

register_command!("BanIpCommands", |ctx| {
    ctx.dispatcher.register(
        literal("ban-ip")
            .requires(has_permission(LEVEL_ADMINS))
            .then(
                argument("target", StringArgumentType::word())
                    .executes(|c| {
                        ban_ip_or_name(
                            c.get_source(),
                            StringArgumentType::get_string(c, "target"),
                            None,
                        )
                    })
                    .then(
                        argument("reason", MessageArgument::message()).executes(|c| {
                            ban_ip_or_name(
                                c.get_source(),
                                StringArgumentType::get_string(c, "target"),
                                Some(MessageArgument::get_message(c, "reason")?),
                            )
                        }),
                    ),
            ),
    );
});

fn ban_ip_or_name(
    source: &CommandSourceStack,
    target: &str,
    reason: Option<Component>,
) -> Result<i32, CommandSyntaxException> {
    if target.parse::<std::net::IpAddr>().is_ok() {
        return ban_ip(source, target, reason);
    }
    match source.with_server(|server| {
        server
            .player_list
            .get_player_by_name(target)
            .map(|p| p.ip_address.clone())
    }) {
        Some(ip) => ban_ip(source, &ip, reason),
        None => Err(ERROR_INVALID_IP.create()),
    }
}

fn ban_ip(
    source: &CommandSourceStack,
    ip: &str,
    reason: Option<Component>,
) -> Result<i32, CommandSyntaxException> {
    let text_name = source.get_text_name();
    let reason_string = reason.as_ref().map(|r| r.get_string());
    let banned = source.with_server(|server| {
        if server.player_list.ip_bans.is_banned(ip) {
            return None;
        }
        let players = server.player_list.get_players_with_address(ip);
        let entry = IpBanListEntry::new(
            Some(ip.to_owned()),
            None,
            Some(&text_name),
            None,
            reason_string.as_deref(),
        );
        server.player_list.ip_bans.add(entry.clone());
        let refs: Vec<EntityRef> = players
            .iter()
            .filter_map(|p| host::player_by_uuid(server, p.uuid))
            .collect();
        let names = EntitySelector::join_names(server, &refs);
        Some((entry, players, names))
    });
    let Some((entry, players, names)) = banned else {
        return Err(ERROR_ALREADY_BANNED.create());
    };
    source.send_success(
        || {
            translatable!(
                "commands.banip.success",
                ip.to_owned(),
                entry.get_reason_message()
            )
        },
        true,
    );
    if !players.is_empty() {
        source.send_success(
            || translatable!("commands.banip.info", players.len() as i32, names),
            true,
        );
    }
    for player in &players {
        player.disconnect(&translatable!("multiplayer.disconnect.ip_banned"));
    }
    Ok(players.len() as i32)
}
