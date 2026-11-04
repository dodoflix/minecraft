//! `SayCommand`.
use crate::commands::arguments::MessageArgument;
use crate::commands::host;
use crate::commands::{LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::register_command;
use mc_brigadier::ArgumentBuilder;
use crate::server::chat::chat_type;

register_command!("SayCommand", |ctx| {
    ctx.dispatcher.register(literal("say").requires(has_permission(LEVEL_GAMEMASTERS)).then(argument("message", MessageArgument::message()).executes(|c| {
        MessageArgument::resolve_chat_message(c, "message", |message| {
            let source = c.get_source();
            let name = source.get_display_name();
            source.with_server(|server| {
                let bound = host::bind_chat_type(server, chat_type::SAY_COMMAND, name);
                let sender = source.get_player().and_then(|p| p.player.clone());
                host::broadcast_chat_message(server, &message, sender.as_ref(), &bound);
            });
        })?;
        Ok(1)
    })));
});
