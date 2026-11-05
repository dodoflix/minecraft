//! `TriggerCommand`.
use super::scoreboard_command::{formatted_name, with_scoreboard};
use crate::commands::arguments::scores::ScoreHolder;
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::ObjectiveArgument;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::{CommandSourceStack, argument, literal};
use crate::world::scores::ObjectiveCriteria;
use crate::{register_command, translatable};
use mc_brigadier::arguments::{IntegerArgumentType, integer};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType, Suggestions, SuggestionsBuilder};
use std::sync::LazyLock;

static ERROR_NOT_PRIMED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.trigger.failed.unprimed"));
static ERROR_INVALID_OBJECTIVE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.trigger.failed.invalid"));

register_command!("TriggerCommand", |ctx| {
    ctx.dispatcher.register(
        literal("trigger").then(
            argument("objective", ObjectiveArgument::objective())
                .suggests(|c, p| Ok(suggest_objectives(c.get_source(), p)))
                .executes(|c| {
                    let player = player(c)?;
                    simple_trigger(c.get_source(), &player, &ObjectiveArgument::get_objective(c, "objective")?)
                })
                .then(literal("add").then(argument("value", integer()).executes(|c| {
                    let player = player(c)?;
                    add_value(c.get_source(), &player, &ObjectiveArgument::get_objective(c, "objective")?, IntegerArgumentType::get_integer(c, "value"))
                })))
                .then(literal("set").then(argument("value", integer()).executes(|c| {
                    let player = player(c)?;
                    set_value(c.get_source(), &player, &ObjectiveArgument::get_objective(c, "objective")?, IntegerArgumentType::get_integer(c, "value"))
                }))),
        ),
    );
});

/// `source.getPlayerOrException()` as a score holder.
fn player(c: &CommandContext<CommandSourceStack>) -> Result<ScoreHolder, CommandSyntaxException> {
    let source = c.get_source();
    let player = EntityRef { dimension: source.get_level().clone(), id: source.get_player_or_exception()?.id };
    Ok(source.with_server(|server| ScoreHolder::of_entity(server, &player)).expect("source player is alive"))
}

/// `TriggerCommand.suggestObjectives`.
pub fn suggest_objectives(source: &CommandSourceStack, builder: SuggestionsBuilder) -> Suggestions {
    let holder = source.get_entity().and_then(|e| {
        let entity = EntityRef { dimension: source.get_level().clone(), id: e.id };
        source.with_server(|server| ScoreHolder::of_entity(server, &entity))
    });
    let result: Vec<String> = match holder {
        None => Vec::new(),
        Some(holder) => source.with_server(|server| {
            let scoreboard = server.scoreboard.read();
            scoreboard
                .get_objectives()
                .filter(|o| *o.get_criteria() == ObjectiveCriteria::TRIGGER)
                .filter(|o| scoreboard.get_player_score_info(&holder, o.get_name()).is_some_and(|info| !info.is_locked()))
                .map(|o| o.get_name().to_owned())
                .collect()
        }),
    };
    suggest(result, builder)
}

/// `getScore(scoreboard, holder, objective)`: checks the objective is a primed trigger and locks it.
fn lock_score(source: &CommandSourceStack, holder: &ScoreHolder, objective: &str) -> Result<(), CommandSyntaxException> {
    let (is_trigger, primed) = source.with_server(|server| {
        let scoreboard = server.scoreboard.read();
        (
            *scoreboard.get_objective(objective).expect("objective exists").get_criteria() == ObjectiveCriteria::TRIGGER,
            scoreboard.get_player_score_info(holder, objective).is_some_and(|info| !info.is_locked()),
        )
    });
    if !is_trigger {
        return Err(ERROR_INVALID_OBJECTIVE.create());
    }
    if !primed {
        return Err(ERROR_NOT_PRIMED.create());
    }
    with_scoreboard(source, |scoreboard| scoreboard.get_or_create_player_score(holder, objective).expect("objective exists").lock());
    Ok(())
}

fn add_value(source: &CommandSourceStack, player: &ScoreHolder, objective: &str, amount: i32) -> Result<i32, CommandSyntaxException> {
    lock_score(source, player, objective)?;
    let new_value = with_scoreboard(source, |scoreboard| {
        scoreboard.get_or_create_player_score(player, objective).expect("objective exists").add(amount).unwrap_or_else(|e| panic!("{e}"))
    });
    let name = formatted_name(source, objective);
    source.send_success(|| translatable!("commands.trigger.add.success", name, amount), true);
    Ok(new_value)
}

fn set_value(source: &CommandSourceStack, player: &ScoreHolder, objective: &str, amount: i32) -> Result<i32, CommandSyntaxException> {
    lock_score(source, player, objective)?;
    with_scoreboard(source, |scoreboard| {
        scoreboard.get_or_create_player_score(player, objective).expect("objective exists").set(amount).unwrap_or_else(|e| panic!("{e}"))
    });
    let name = formatted_name(source, objective);
    source.send_success(|| translatable!("commands.trigger.set.success", name, amount), true);
    Ok(amount)
}

fn simple_trigger(source: &CommandSourceStack, player: &ScoreHolder, objective: &str) -> Result<i32, CommandSyntaxException> {
    lock_score(source, player, objective)?;
    let new_value = with_scoreboard(source, |scoreboard| {
        scoreboard.get_or_create_player_score(player, objective).expect("objective exists").add(1).unwrap_or_else(|e| panic!("{e}"))
    });
    let name = formatted_name(source, objective);
    source.send_success(|| translatable!("commands.trigger.simple.success", name), true);
    Ok(new_value)
}
