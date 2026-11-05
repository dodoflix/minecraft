//! `StopwatchCommand`.
use crate::commands::arguments::IdentifierArgument;
use crate::commands::message::message;
use crate::commands::shared_suggestion_provider::suggest_resource;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::world::level::saveddata::stopwatches::{Stopwatch, Stopwatches};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{DoubleArgumentType, double_arg};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, DynamicCommandExceptionType, SuggestionProvider};
use mc_core::ident::Identifier;
use mc_text::Component;
use std::sync::{Arc, LazyLock};

static ERROR_ALREADY_EXISTS: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|id: String| message(translatable!("commands.stopwatch.already_exists", id))));
/// `StopwatchCommand.ERROR_DOES_NOT_EXIST` (public in vanilla).
pub static ERROR_DOES_NOT_EXIST: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|id: String| message(translatable!("commands.stopwatch.does_not_exist", id))));

/// `StopwatchCommand.SUGGEST_STOPWATCHES`.
pub fn suggest_stopwatches() -> SuggestionProvider<CommandSourceStack> {
    Arc::new(|context, builder| Ok(suggest_resource(with_stopwatches(context.get_source(), |s| s.ids()), builder)))
}

fn with_stopwatches<R>(source: &CommandSourceStack, f: impl FnOnce(&mut Stopwatches) -> R) -> R {
    source.with_server(|server| f(server.saved_data.compute_if_absent(&server.saved_types.stopwatches)))
}

register_command!("StopwatchCommand", |ctx| {
    let id = |c: &CommandContext<CommandSourceStack>| IdentifierArgument::get_id(c, "id");
    ctx.dispatcher.register(
        literal("stopwatch")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("create").then(argument("id", IdentifierArgument::id()).executes(move |c| create_stopwatch(c.get_source(), id(c)))))
            .then(
                literal("query").then(
                    argument("id", IdentifierArgument::id())
                        .suggests_provider(Some(suggest_stopwatches()))
                        .then(argument("scale", double_arg()).executes(move |c| query_stopwatch(c.get_source(), id(c), DoubleArgumentType::get_double(c, "scale"))))
                        .executes(move |c| query_stopwatch(c.get_source(), id(c), 1.0)),
                ),
            )
            .then(literal("restart").then(argument("id", IdentifierArgument::id()).suggests_provider(Some(suggest_stopwatches())).executes(move |c| restart_stopwatch(c.get_source(), id(c)))))
            .then(literal("remove").then(argument("id", IdentifierArgument::id()).suggests_provider(Some(suggest_stopwatches())).executes(move |c| remove_stopwatch(c.get_source(), id(c))))),
    );
});

fn create_stopwatch(source: &CommandSourceStack, id: Identifier) -> Result<i32, CommandSyntaxException> {
    if !with_stopwatches(source, |s| s.add(id.clone(), Stopwatch::new(Stopwatches::current_time()))) {
        return Err(ERROR_ALREADY_EXISTS.create(id.to_string()));
    }
    source.send_success(|| translatable!("commands.stopwatch.create.success", Component::literal(id.to_string())), true);
    Ok(1)
}

fn query_stopwatch(source: &CommandSourceStack, id: Identifier, scale: f64) -> Result<i32, CommandSyntaxException> {
    let Some(stopwatch) = with_stopwatches(source, |s| s.get(&id).copied()) else { return Err(ERROR_DOES_NOT_EXIST.create(id.to_string())) };
    let elapsed_seconds = stopwatch.elapsed_seconds(Stopwatches::current_time());
    source.send_success(|| translatable!("commands.stopwatch.query", Component::literal(id.to_string()), elapsed_seconds), true);
    Ok((elapsed_seconds * scale) as i32)
}

fn restart_stopwatch(source: &CommandSourceStack, id: Identifier) -> Result<i32, CommandSyntaxException> {
    if !with_stopwatches(source, |s| s.update(&id, |_| Stopwatch::new(Stopwatches::current_time()))) {
        return Err(ERROR_DOES_NOT_EXIST.create(id.to_string()));
    }
    source.send_success(|| translatable!("commands.stopwatch.restart.success", Component::literal(id.to_string())), true);
    Ok(1)
}

fn remove_stopwatch(source: &CommandSourceStack, id: Identifier) -> Result<i32, CommandSyntaxException> {
    if !with_stopwatches(source, |s| s.remove(&id)) {
        return Err(ERROR_DOES_NOT_EXIST.create(id.to_string()));
    }
    source.send_success(|| translatable!("commands.stopwatch.remove.success", Component::literal(id.to_string())), true);
    Ok(1)
}
