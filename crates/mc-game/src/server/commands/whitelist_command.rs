//! `WhitelistCommand`.
use crate::commands::arguments::GameProfileArgument;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, host, literal};
use crate::server::players::{NameAndId, UserWhiteListEntry};
use crate::{register_command, translatable};
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_ALREADY_ENABLED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.whitelist.alreadyOn"));
static ERROR_ALREADY_DISABLED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.whitelist.alreadyOff"));
static ERROR_ALREADY_WHITELISTED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.whitelist.add.failed"));
static ERROR_NOT_WHITELISTED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.whitelist.remove.failed"));

register_command!("WhitelistCommand", |ctx| {
    ctx.dispatcher.register(
        literal("whitelist")
            .requires(has_permission(LEVEL_ADMINS))
            .then(literal("on").executes(|c| enable_whitelist(c.get_source())))
            .then(literal("off").executes(|c| disable_whitelist(c.get_source())))
            .then(literal("list").executes(|c| Ok(show_list(c.get_source()))))
            .then(literal("add").then(
                argument("targets", GameProfileArgument::game_profile())
                    .suggests(|c, p| {
                        let names = c.get_source().with_server(|server| {
                            let list = &server.player_list;
                            list.get_players().iter().map(|p| p.name_and_id()).filter(|n| !list.whitelist.is_white_listed(n)).map(|n| n.name).collect::<Vec<_>>()
                        });
                        Ok(suggest(names, p))
                    })
                    .executes(|c| add_players(c.get_source(), GameProfileArgument::get_game_profiles(c, "targets")?)),
            ))
            .then(literal("remove").then(
                argument("targets", GameProfileArgument::game_profile())
                    .suggests(|c, p| Ok(suggest(c.get_source().with_server(|server| server.player_list.whitelist.get_user_list()), p)))
                    .executes(|c| remove_players(c.get_source(), GameProfileArgument::get_game_profiles(c, "targets")?)),
            ))
            .then(literal("reload").executes(|c| Ok(reload(c.get_source())))),
    );
});

fn reload(source: &CommandSourceStack) -> i32 {
    source.with_server(|server| server.player_list.reload_white_list());
    source.send_success(|| translatable!("commands.whitelist.reloaded"), true);
    source.with_server(host::kick_unlisted_players);
    1
}

fn add_players(source: &CommandSourceStack, targets: Vec<NameAndId>) -> Result<i32, CommandSyntaxException> {
    let mut success = 0;
    for target in targets {
        let added = source.with_server(|server| !server.player_list.whitelist.is_white_listed(&target) && server.player_list.whitelist.add(UserWhiteListEntry::new(target.clone())));
        if added {
            source.send_success(|| translatable!("commands.whitelist.add.success", Component::literal(target.name.clone())), true);
            success += 1;
        }
    }
    if success == 0 { Err(ERROR_ALREADY_WHITELISTED.create()) } else { Ok(success) }
}

fn remove_players(source: &CommandSourceStack, targets: Vec<NameAndId>) -> Result<i32, CommandSyntaxException> {
    let mut success = 0;
    for target in targets {
        let removed = source.with_server(|server| server.player_list.whitelist.is_white_listed(&target) && server.player_list.whitelist.remove(&target));
        if removed {
            source.send_success(|| translatable!("commands.whitelist.remove.success", Component::literal(target.name.clone())), true);
            success += 1;
        }
    }
    if success == 0 {
        Err(ERROR_NOT_WHITELISTED.create())
    } else {
        source.with_server(host::kick_unlisted_players);
        Ok(success)
    }
}

fn enable_whitelist(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|server| server.player_list.is_using_whitelist()) {
        return Err(ERROR_ALREADY_ENABLED.create());
    }
    source.with_server(|server| server.set_using_whitelist(true));
    source.send_success(|| translatable!("commands.whitelist.enabled"), true);
    source.with_server(host::kick_unlisted_players);
    Ok(1)
}

fn disable_whitelist(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if !source.with_server(|server| server.player_list.is_using_whitelist()) {
        return Err(ERROR_ALREADY_DISABLED.create());
    }
    source.with_server(|server| server.set_using_whitelist(false));
    source.send_success(|| translatable!("commands.whitelist.disabled"), true);
    Ok(1)
}

fn show_list(source: &CommandSourceStack) -> i32 {
    let list = source.with_server(|server| server.player_list.whitelist.get_user_list());
    if list.is_empty() {
        source.send_success(|| translatable!("commands.whitelist.none"), false);
    } else {
        source.send_success(|| translatable!("commands.whitelist.list", list.len() as i32, list.join(", ")), false);
    }
    list.len() as i32
}
