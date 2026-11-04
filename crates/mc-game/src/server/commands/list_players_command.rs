//! `ListPlayersCommand`.
use crate::commands::{CommandSourceStack, host, literal};
use crate::{register_command, translatable};
use mc_text::Component;
use mc_text::util::{default_separator, format_list};

register_command!("ListPlayersCommand", |ctx| {
    ctx.dispatcher.register(
        literal("list").executes(|c| Ok(list_players(c.get_source()))).then(literal("uuids").executes(|c| Ok(list_players_with_uuids(c.get_source())))),
    );
});

fn list_players(source: &CommandSourceStack) -> i32 {
    format(source, |server, p| host::player_display_name(server, p))
}

fn list_players_with_uuids(source: &CommandSourceStack) -> i32 {
    format(source, |_, p| {
        translatable!("commands.list.nameAndId", Component::literal(p.name().to_owned()), Component::literal(p.uuid.to_string()))
    })
}

fn format(source: &CommandSourceStack, formatter: impl Fn(&crate::server::MinecraftServer, &crate::server::players::player_handle::PlayerHandle) -> Component) -> i32 {
    let (components, max_players) = source.with_server(|server| {
        let players = server.player_list.get_players();
        (players.iter().map(|p| formatter(server, p)).collect::<Vec<_>>(), server.player_list.get_max_players())
    });
    let count = components.len();
    let list = format_list(components, &default_separator(), |c| c);
    source.send_success(|| translatable!("commands.list.players", count as i32, max_players, list), false);
    count as i32
}
