//! `HelpCommand`.
use crate::commands::message::simple_translatable;
use crate::commands::{argument, literal};
use crate::register_command;
use mc_brigadier::{ImmutableStringReader, SimpleCommandExceptionType, StringArgumentType};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.help.failed"));

register_command!("HelpCommand", |ctx| {
    let dispatcher = ctx.dispatcher;
    // The closures need the dispatcher: vanilla's lambdas capture `dispatcher`; the root node is shared (`Arc`).
    let root = dispatcher.get_root().clone();
    let root2 = root.clone();
    let dispatcher_for_parse = mc_brigadier::CommandDispatcher::with_root(root.clone());
    let dispatcher_for_usage = mc_brigadier::CommandDispatcher::with_root(root.clone());
    dispatcher.register(
        literal("help")
            .executes(move |s| {
                let usage = dispatcher_for_usage.get_smart_usage(&root2, s.get_source());
                for (_, line) in &usage {
                    s.get_source().send_success(|| Component::literal(format!("/{line}")), false);
                }
                Ok(usage.len() as i32)
            })
            .then(argument("command", StringArgumentType::greedy_string()).executes(move |s| {
                let command = dispatcher_for_parse.parse(StringArgumentType::get_string(s, "command"), s.get_source().clone());
                let Some(last) = command.get_context().get_nodes().last() else { return Err(ERROR_FAILED.create()) };
                let usage = dispatcher_for_parse.get_smart_usage(last.get_node(), s.get_source());
                for (_, line) in &usage {
                    s.get_source().send_success(|| Component::literal(format!("/{} {line}", command.get_reader().get_string())), false);
                }
                Ok(usage.len() as i32)
            })),
    );
});
