//! `HelpCommand`.
use crate::commands::message::simple_translatable;
use crate::commands::{argument, literal};
use crate::register_command;
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::ImmutableStringReader;
use mc_brigadier::{SimpleCommandExceptionType};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.help.failed"));

// The closures reach the dispatcher through the server (vanilla's lambdas capture the dispatcher they are registered in).
register_command!("HelpCommand", |ctx| {
    ctx.dispatcher.register(
        literal("help")
            .executes(|s| {
                let commands = s.get_source().with_server(|server| server.commands.clone());
                let dispatcher = commands.get_dispatcher();
                let usage = dispatcher.get_smart_usage(dispatcher.get_root(), s.get_source());
                for (_, line) in &usage {
                    s.get_source().send_success(|| Component::literal(format!("/{line}")), false);
                }
                Ok(usage.len() as i32)
            })
            .then(argument("command", StringArgumentType::greedy_string()).executes(|s| {
                let commands = s.get_source().with_server(|server| server.commands.clone());
                let dispatcher = commands.get_dispatcher();
                let command = dispatcher.parse(StringArgumentType::get_string(s, "command"), s.get_source().clone());
                let Some(last) = command.get_context().get_nodes().last() else { return Err(ERROR_FAILED.create()) };
                let usage = dispatcher.get_smart_usage(last.get_node(), s.get_source());
                for (_, line) in &usage {
                    s.get_source().send_success(|| Component::literal(format!("/{} {line}", command.get_reader().get_string())), false);
                }
                Ok(usage.len() as i32)
            })),
    );
});
