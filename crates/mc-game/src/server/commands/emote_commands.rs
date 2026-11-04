//! `EmoteCommands`.
use crate::commands::arguments::MessageArgument;
use crate::commands::host;
use crate::commands::{argument, literal};
use crate::register_command;
use crate::server::chat::chat_type;

register_command!("EmoteCommands", |ctx| {
    ctx.dispatcher.register(literal("me").then(argument("action", MessageArgument::message()).executes(|c| {
        MessageArgument::resolve_chat_message(c, "action", |message| {
            let source = c.get_source();
            let name = source.get_display_name();
            source.with_server(|server| {
                let bound = host::bind_chat_type(server, chat_type::EMOTE_COMMAND, name);
                host::broadcast_chat_message(server, &message, source, &bound);
            });
        })?;
        Ok(1)
    })));
});
