//! `BanListCommands`.
use crate::commands::{CommandSourceStack, LEVEL_ADMINS, has_permission, literal};
use mc_brigadier::ArgumentBuilder;
use crate::{register_command, translatable};
use mc_text::Component;

/// `(display name, source, reason message)` of a ban list entry.
type Entry = (Component, String, Component);

register_command!("BanListCommands", |ctx| {
    ctx.dispatcher.register(
        literal("banlist")
            .requires(has_permission(LEVEL_ADMINS))
            .executes(|s| {
                let mut list = users(s.get_source());
                list.extend(ips(s.get_source()));
                Ok(show_list(s.get_source(), list))
            })
            .then(literal("ips").executes(|s| Ok(show_list(s.get_source(), ips(s.get_source())))))
            .then(literal("players").executes(|s| Ok(show_list(s.get_source(), users(s.get_source()))))),
    );
});

fn users(source: &CommandSourceStack) -> Vec<Entry> {
    source.with_server(|server| {
        server.player_list.bans.get_entries().map(|e| (e.get_display_name(), e.source.clone(), e.get_reason_message())).collect()
    })
}

fn ips(source: &CommandSourceStack) -> Vec<Entry> {
    source.with_server(|server| {
        server.player_list.ip_bans.get_entries().map(|e| (e.get_display_name(), e.source.clone(), e.get_reason_message())).collect()
    })
}

fn show_list(source: &CommandSourceStack, list: Vec<Entry>) -> i32 {
    if list.is_empty() {
        source.send_success(|| translatable!("commands.banlist.none"), false);
    } else {
        let size = list.len() as i32;
        source.send_success(|| translatable!("commands.banlist.list", size), false);
        for (display_name, entry_source, reason) in list.iter().cloned() {
            source.send_success(|| translatable!("commands.banlist.entry", display_name, entry_source, reason), false);
        }
    }
    list.len() as i32
}
