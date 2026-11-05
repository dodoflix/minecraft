//! `ScoreboardCommand`.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use crate::commands::arguments::scores::{ScoreHolder, suggest_score_holders};
use crate::commands::arguments::{
    ComponentArgument, ObjectiveArgument, ObjectiveCriteriaArgument, OperationArgument, ScoreHolderArgument, ScoreboardSlotArgument, StyleArgument,
};
use crate::commands::message::{message, simple_translatable};
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::server::MinecraftServer;
use crate::world::scores::scoreboard::DetachedScoreAccess;
use crate::world::scores::{DisplaySlot, NumberFormat, ObjectiveCriteria, RenderType, ScoreAccess, Scoreboard, ScoreboardError};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{BoolArgumentType, IntegerArgumentType, StringArgumentType, integer, integer_min};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, SimpleCommandExceptionType};
use mc_text::util::{default_separator, format_list};
use mc_text::Component;
use std::sync::{Arc, LazyLock};

static ERROR_OBJECTIVE_ALREADY_EXISTS: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.scoreboard.objectives.add.duplicate"));
static ERROR_DISPLAY_SLOT_ALREADY_EMPTY: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.scoreboard.objectives.display.alreadyEmpty"));
static ERROR_DISPLAY_SLOT_ALREADY_SET: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.scoreboard.objectives.display.alreadySet"));
static ERROR_TRIGGER_ALREADY_ENABLED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.scoreboard.players.enable.failed"));
static ERROR_NOT_TRIGGER: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.scoreboard.players.enable.invalid"));
static ERROR_NO_VALUE: LazyLock<Dynamic2CommandExceptionType<String, Component>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|objective: String, target: Component| message(translatable!("commands.scoreboard.players.get.null", objective, target)))
});

/// `NumberFormatCommandExecutor`.
type NumberFormatExecutor = Arc<dyn Fn(&CommandContext<CommandSourceStack>, Option<NumberFormat>) -> Result<i32, CommandSyntaxException> + Send + Sync>;

register_command!("ScoreboardCommand", |ctx| {
    fn objective(c: &CommandContext<CommandSourceStack>, name: &str) -> Result<String, CommandSyntaxException> {
        ObjectiveArgument::get_objective(c, name)
    }
    fn targets(c: &CommandContext<CommandSourceStack>, name: &str) -> Result<Vec<ScoreHolder>, CommandSyntaxException> {
        ScoreHolderArgument::get_names_with_default_wildcard(c, name)
    }
    fn holders<B: ArgumentBuilder<CommandSourceStack>>(builder: mc_brigadier::RequiredArgumentBuilder<CommandSourceStack>, then: B) -> mc_brigadier::RequiredArgumentBuilder<CommandSourceStack> {
        builder.suggests_provider(Some(suggest_score_holders())).then(then)
    }

    let mut render_type = literal("rendertype");
    for rt in [RenderType::Integer, RenderType::Hearts] {
        render_type = render_type.then(
            literal(rt.get_id()).executes(move |c| set_render_type(c.get_source(), &ObjectiveArgument::get_objective(c, "objective")?, rt)),
        );
    }

    ctx.dispatcher.register(
        literal("scoreboard")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("objectives")
                    .then(literal("list").executes(|c| Ok(list_objectives(c.get_source()))))
                    .then(literal("add").then(argument("objective", StringArgumentType::word()).then(
                        argument("criteria", ObjectiveCriteriaArgument::criteria())
                            .executes(|c| {
                                let name = StringArgumentType::get_string(c, "objective").to_owned();
                                add_objective(c.get_source(), &name, ObjectiveCriteriaArgument::get_criteria(c, "criteria"), Component::literal(name.clone()))
                            })
                            .then(argument("displayName", ComponentArgument::text_component()).executes(|c| {
                                add_objective(
                                    c.get_source(),
                                    StringArgumentType::get_string(c, "objective"),
                                    ObjectiveCriteriaArgument::get_criteria(c, "criteria"),
                                    ComponentArgument::get_resolved_component_for_source(c, "displayName")?,
                                )
                            })),
                    )))
                    .then(
                        literal("modify").then(
                            argument("objective", ObjectiveArgument::objective())
                                .then(literal("displayname").then(argument("displayName", ComponentArgument::text_component()).executes(|c| {
                                    set_display_name(c.get_source(), &objective(c, "objective")?, ComponentArgument::get_resolved_component_for_source(c, "displayName")?)
                                })))
                                .then(render_type)
                                .then(literal("displayautoupdate").then(argument("value", BoolArgumentType::bool()).executes(|c| {
                                    set_display_auto_update(c.get_source(), &objective(c, "objective")?, BoolArgumentType::get_bool(c, "value"))
                                })))
                                .then(add_number_formats(
                                    literal("numberformat"),
                                    Arc::new(|c, format| set_objective_format(c.get_source(), &objective(c, "objective")?, format)),
                                )),
                        ),
                    )
                    .then(literal("remove").then(argument("objective", ObjectiveArgument::objective()).executes(|c| remove_objective(c.get_source(), &objective(c, "objective")?))))
                    .then(literal("setdisplay").then(
                        argument("slot", ScoreboardSlotArgument::display_slot())
                            .executes(|c| clear_display_slot(c.get_source(), ScoreboardSlotArgument::get_display_slot(c, "slot")))
                            .then(argument("objective", ObjectiveArgument::objective()).executes(|c| {
                                set_display_slot(c.get_source(), ScoreboardSlotArgument::get_display_slot(c, "slot"), &objective(c, "objective")?)
                            })),
                    )),
            )
            .then(
                literal("players")
                    .then(
                        literal("list").executes(|c| Ok(list_tracked_players(c.get_source()))).then(
                            argument("target", ScoreHolderArgument::score_holder())
                                .suggests_provider(Some(suggest_score_holders()))
                                .executes(|c| Ok(list_tracked_player_scores(c.get_source(), &ScoreHolderArgument::get_name(c, "target")?))),
                        ),
                    )
                    .then(literal("set").then(holders(
                        argument("targets", ScoreHolderArgument::score_holders()),
                        argument("objective", ObjectiveArgument::objective()).then(argument("score", integer()).executes(|c| {
                            set_score(
                                c.get_source(),
                                targets(c, "targets")?,
                                &ObjectiveArgument::get_writable_objective(c, "objective")?,
                                IntegerArgumentType::get_integer(c, "score"),
                            )
                        })),
                    )))
                    .then(literal("get").then(
                        argument("target", ScoreHolderArgument::score_holder()).suggests_provider(Some(suggest_score_holders())).then(
                            argument("objective", ObjectiveArgument::objective())
                                .executes(|c| get_score(c.get_source(), ScoreHolderArgument::get_name(c, "target")?, &objective(c, "objective")?)),
                        ),
                    ))
                    .then(literal("add").then(holders(
                        argument("targets", ScoreHolderArgument::score_holders()),
                        argument("objective", ObjectiveArgument::objective()).then(argument("score", integer_min(0)).executes(|c| {
                            add_score(
                                c.get_source(),
                                targets(c, "targets")?,
                                &ObjectiveArgument::get_writable_objective(c, "objective")?,
                                IntegerArgumentType::get_integer(c, "score"),
                            )
                        })),
                    )))
                    .then(literal("remove").then(holders(
                        argument("targets", ScoreHolderArgument::score_holders()),
                        argument("objective", ObjectiveArgument::objective()).then(argument("score", integer_min(0)).executes(|c| {
                            remove_score(
                                c.get_source(),
                                targets(c, "targets")?,
                                &ObjectiveArgument::get_writable_objective(c, "objective")?,
                                IntegerArgumentType::get_integer(c, "score"),
                            )
                        })),
                    )))
                    .then(literal("reset").then(
                        argument("targets", ScoreHolderArgument::score_holders())
                            .suggests_provider(Some(suggest_score_holders()))
                            .executes(|c| reset_scores(c.get_source(), targets(c, "targets")?))
                            .then(argument("objective", ObjectiveArgument::objective()).executes(|c| {
                                reset_score(c.get_source(), targets(c, "targets")?, &objective(c, "objective")?)
                            })),
                    ))
                    .then(literal("enable").then(
                        argument("targets", ScoreHolderArgument::score_holders()).suggests_provider(Some(suggest_score_holders())).then(
                            argument("objective", ObjectiveArgument::objective())
                                .suggests(|c, p| Ok(suggest_triggers(c.get_source(), &targets(c, "targets")?, p)))
                                .executes(|c| enable_trigger(c.get_source(), targets(c, "targets")?, &objective(c, "objective")?)),
                        ),
                    ))
                    .then(
                        literal("display")
                            .then(literal("name").then(holders(
                                argument("targets", ScoreHolderArgument::score_holders()),
                                argument("objective", ObjectiveArgument::objective())
                                    .then(argument("name", ComponentArgument::text_component()).executes(|c| {
                                        set_score_display(
                                            c.get_source(),
                                            targets(c, "targets")?,
                                            &objective(c, "objective")?,
                                            Some(ComponentArgument::get_resolved_component_for_source(c, "name")?),
                                        )
                                    }))
                                    .executes(|c| set_score_display(c.get_source(), targets(c, "targets")?, &objective(c, "objective")?, None)),
                            )))
                            .then(literal("numberformat").then(holders(
                                argument("targets", ScoreHolderArgument::score_holders()),
                                add_number_formats(
                                    argument("objective", ObjectiveArgument::objective()),
                                    Arc::new(|c, format| set_score_number_format(c.get_source(), targets(c, "targets")?, &objective(c, "objective")?, format)),
                                ),
                            ))),
                    )
                    .then(literal("operation").then(holders(
                        argument("targets", ScoreHolderArgument::score_holders()),
                        argument("targetObjective", ObjectiveArgument::objective()).then(argument("operation", OperationArgument::operation()).then(holders(
                            argument("source", ScoreHolderArgument::score_holders()),
                            argument("sourceObjective", ObjectiveArgument::objective()).executes(|c| {
                                perform_operation(
                                    c.get_source(),
                                    targets(c, "targets")?,
                                    &ObjectiveArgument::get_writable_objective(c, "targetObjective")?,
                                    OperationArgument::get_operation(c, "operation"),
                                    targets(c, "source")?,
                                    &objective(c, "sourceObjective")?,
                                )
                            }),
                        ))),
                    ))),
            ),
    );
});

/// `addNumberFormats(context, top, callback)`.
fn add_number_formats<B: ArgumentBuilder<CommandSourceStack>>(top: B, callback: NumberFormatExecutor) -> B {
    let (blank, fixed, styled, none) = (callback.clone(), callback.clone(), callback.clone(), callback);
    top.then(literal("blank").executes(move |c| blank(c, Some(NumberFormat::Blank))))
        .then(literal("fixed").then(argument("contents", ComponentArgument::text_component()).executes(move |c| {
            let contents = ComponentArgument::get_resolved_component_for_source(c, "contents")?;
            fixed(c, Some(NumberFormat::Fixed(contents)))
        })))
        .then(literal("styled").then(argument("style", StyleArgument::style()).executes(move |c| {
            let style = StyleArgument::get_style(c, "style");
            styled(c, Some(NumberFormat::Styled(style)))
        })))
        .executes(move |c| none(c, None))
}

/// Java exceptions of the score model (`IllegalStateException`): they fail the whole command (`command.failed`).
fn expect_ok<T>(result: Result<T, ScoreboardError>) -> T {
    result.unwrap_or_else(|e| panic!("{e}"))
}

/// Runs `f` on the scoreboard, then sends the packets it queued (vanilla broadcasts them at each change, before the
/// command's feedback).
pub(super) fn with_scoreboard<R>(source: &CommandSourceStack, f: impl FnOnce(&mut Scoreboard) -> R) -> R {
    source.with_server(|server: &mut MinecraftServer| {
        let result = f(&mut server.scoreboard.write());
        server.flush_scoreboard();
        result
    })
}

/// `getOrCreatePlayerScore(holder, objective)` (the objective exists: the argument checked it).
fn score_access<'a>(scoreboard: &'a mut Scoreboard, holder: &ScoreHolder, objective: &str) -> ScoreAccess<'a> {
    scoreboard.get_or_create_player_score(holder, objective).expect("objective exists")
}

/// `objective.getFormattedDisplayName()`.
pub(super) fn formatted_name(source: &CommandSourceStack, objective: &str) -> Component {
    source.with_server(|s| s.scoreboard.read().get_objective(objective).expect("objective exists").get_formatted_display_name().clone())
}

fn suggest_triggers(
    source: &CommandSourceStack,
    targets: &[ScoreHolder],
    builder: mc_brigadier::SuggestionsBuilder,
) -> mc_brigadier::Suggestions {
    let result: Vec<String> = source.with_server(|server| {
        let scoreboard = server.scoreboard.read();
        scoreboard
            .get_objectives()
            .filter(|o| *o.get_criteria() == ObjectiveCriteria::TRIGGER)
            .filter(|o| targets.iter().any(|t| scoreboard.get_player_score_info(t, o.get_name()).is_none_or(|info| info.is_locked())))
            .map(|o| o.get_name().to_owned())
            .collect()
    });
    suggest(result, builder)
}

fn get_score(source: &CommandSourceStack, target: ScoreHolder, objective: &str) -> Result<i32, CommandSyntaxException> {
    let score = source.with_server(|s| s.scoreboard.read().get_player_score_info(&target, objective).map(|info| info.value()));
    let Some(value) = score else { return Err(ERROR_NO_VALUE.create(objective.to_owned(), target.get_feedback_display_name())) };
    let objective_name = formatted_name(source, objective);
    source.send_success(|| translatable!("commands.scoreboard.players.get.success", target.get_feedback_display_name(), value, objective_name), false);
    Ok(value)
}

fn perform_operation(
    source: &CommandSourceStack,
    targets: Vec<ScoreHolder>,
    target_objective: &str,
    operation: crate::commands::arguments::Operation,
    sources: Vec<ScoreHolder>,
    source_objective: &str,
) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    /// Uses a detached access for one call (two accesses are alive at once, like in vanilla).
    fn with(access: &mut DetachedScoreAccess, scoreboard: &mut Scoreboard, f: impl FnOnce(&mut ScoreAccess<'_>)) {
        let mut attached = access.clone().attach(scoreboard);
        f(&mut attached);
        *access = attached.detach();
    }
    let outcome = source.with_server(|server| {
        let mut guard = server.scoreboard.write();
        let scoreboard: &mut Scoreboard = &mut guard;
        let mut result = Ok(());
        'targets: for target in &targets {
            let mut score = score_access(scoreboard, target, target_objective).detach();
            for from in &sources {
                let mut source_score = score_access(scoreboard, from, source_objective).detach();
                let (mut a, mut b) = (0, 0);
                with(&mut score, scoreboard, |s| a = s.get());
                with(&mut source_score, scoreboard, |s| b = s.get());
                match operation.apply(a, b) {
                    Ok((new_a, new_b)) => {
                        with(&mut score, scoreboard, |s| expect_ok(s.set(new_a)));
                        if let Some(new_b) = new_b {
                            with(&mut source_score, scoreboard, |s| expect_ok(s.set(new_b)));
                        }
                    }
                    Err(e) => {
                        result = Err(e);
                        break 'targets;
                    }
                }
            }
            let mut value = 0;
            with(&mut score, scoreboard, |s| value = s.get());
            tracker.track_value(target.clone(), value);
        }
        drop(guard);
        server.flush_scoreboard();
        result
    });
    outcome?;
    let objective = formatted_name(source, target_objective);
    tracker.send_feedback_with(
        source,
        true,
        ElementType::Any,
        None,
        |holder, total| translatable!("commands.scoreboard.players.operation.success.single", objective.clone(), holder.get_feedback_display_name(), total),
        |count, _| translatable!("commands.scoreboard.players.operation.success.multiple", objective.clone(), count),
    )
}

fn enable_trigger(source: &CommandSourceStack, targets: Vec<ScoreHolder>, objective: &str) -> Result<i32, CommandSyntaxException> {
    let is_trigger = source.with_server(|s| *s.scoreboard.read().get_objective(objective).expect("objective exists").get_criteria() == ObjectiveCriteria::TRIGGER);
    if !is_trigger {
        return Err(ERROR_NOT_TRIGGER.create());
    }
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            let mut score = score_access(scoreboard, target, objective);
            if score.locked() {
                score.unlock();
                tracker.track(target.clone());
            }
        }
    });
    let objective = formatted_name(source, objective);
    tracker.send_feedback(
        source,
        true,
        Some(ERROR_TRIGGER_ALREADY_ENABLED.create()),
        |holder, _| translatable!("commands.scoreboard.players.enable.success.single", objective.clone(), holder.get_feedback_display_name()),
        |count, _| translatable!("commands.scoreboard.players.enable.success.multiple", objective.clone(), count),
    )
}

fn reset_scores(source: &CommandSourceStack, targets: Vec<ScoreHolder>) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            scoreboard.reset_all_player_scores(target);
            tracker.track(target.clone());
        }
    });
    tracker.send_feedback(
        source,
        true,
        None,
        |holder, _| translatable!("commands.scoreboard.players.reset.all.single", holder.get_feedback_display_name()),
        |count, _| translatable!("commands.scoreboard.players.reset.all.multiple", count),
    )
}

fn reset_score(source: &CommandSourceStack, targets: Vec<ScoreHolder>, objective: &str) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            scoreboard.reset_single_player_score(target, objective);
            tracker.track(target.clone());
        }
    });
    let objective = formatted_name(source, objective);
    tracker.send_feedback(
        source,
        true,
        None,
        |holder, _| translatable!("commands.scoreboard.players.reset.specific.single", objective.clone(), holder.get_feedback_display_name()),
        |count, _| translatable!("commands.scoreboard.players.reset.specific.multiple", objective.clone(), count),
    )
}

fn set_score(source: &CommandSourceStack, targets: Vec<ScoreHolder>, objective: &str, value: i32) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            expect_ok(score_access(scoreboard, target, objective).set(value));
            tracker.track_value(target.clone(), value);
        }
    });
    let objective = formatted_name(source, objective);
    tracker.send_feedback(
        source,
        true,
        None,
        |holder, _| translatable!("commands.scoreboard.players.set.success.single", objective.clone(), holder.get_feedback_display_name(), value),
        |count, _| translatable!("commands.scoreboard.players.set.success.multiple", objective.clone(), count, value),
    )
}

fn set_score_display(source: &CommandSourceStack, targets: Vec<ScoreHolder>, objective: &str, display: Option<Component>) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            score_access(scoreboard, target, objective).set_display(display.clone());
            tracker.track(target.clone());
        }
    });
    let objective = formatted_name(source, objective);
    match display {
        None => tracker.send_feedback(
            source,
            true,
            None,
            |holder, _| translatable!("commands.scoreboard.players.display.name.clear.success.single", holder.get_feedback_display_name(), objective.clone()),
            |count, _| translatable!("commands.scoreboard.players.display.name.clear.success.multiple", count, objective.clone()),
        ),
        Some(display) => tracker.send_feedback(
            source,
            true,
            None,
            |holder, _| {
                translatable!("commands.scoreboard.players.display.name.set.success.single", display.clone(), holder.get_feedback_display_name(), objective.clone())
            },
            |count, _| translatable!("commands.scoreboard.players.display.name.set.success.multiple", display.clone(), count, objective.clone()),
        ),
    }
}

fn set_score_number_format(
    source: &CommandSourceStack,
    targets: Vec<ScoreHolder>,
    objective: &str,
    number_format: Option<NumberFormat>,
) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    let clearing = number_format.is_none();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            score_access(scoreboard, target, objective).number_format_override(number_format.clone());
            tracker.track(target.clone());
        }
    });
    let objective = formatted_name(source, objective);
    let (single, multiple) = if clearing {
        ("commands.scoreboard.players.display.numberFormat.clear.success.single", "commands.scoreboard.players.display.numberFormat.clear.success.multiple")
    } else {
        ("commands.scoreboard.players.display.numberFormat.set.success.single", "commands.scoreboard.players.display.numberFormat.set.success.multiple")
    };
    tracker.send_feedback(
        source,
        true,
        None,
        |holder, _| translatable!(single, holder.get_feedback_display_name(), objective.clone()),
        |count, _| translatable!(multiple, count, objective.clone()),
    )
}

fn add_score(source: &CommandSourceStack, targets: Vec<ScoreHolder>, objective: &str, value: i32) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            let mut score = score_access(scoreboard, target, objective);
            let new_value = score.get().wrapping_add(value);
            expect_ok(score.set(new_value));
            tracker.track_value(target.clone(), score.get());
        }
    });
    let objective = formatted_name(source, objective);
    tracker.send_feedback_with(
        source,
        true,
        ElementType::Any,
        None,
        |holder, total| translatable!("commands.scoreboard.players.add.success.single", value, objective.clone(), holder.get_feedback_display_name(), total),
        |count, _| translatable!("commands.scoreboard.players.add.success.multiple", value, objective.clone(), count),
    )
}

fn remove_score(source: &CommandSourceStack, targets: Vec<ScoreHolder>, objective: &str, value: i32) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for target in &targets {
            let mut score = score_access(scoreboard, target, objective);
            let new_value = score.get().wrapping_sub(value);
            expect_ok(score.set(new_value));
            tracker.track_value(target.clone(), score.get());
        }
    });
    let objective = formatted_name(source, objective);
    tracker.send_feedback_with(
        source,
        true,
        ElementType::Any,
        None,
        |holder, total| translatable!("commands.scoreboard.players.remove.success.single", value, objective.clone(), holder.get_feedback_display_name(), total),
        |count, _| translatable!("commands.scoreboard.players.remove.success.multiple", value, objective.clone(), count),
    )
}

fn list_tracked_players(source: &CommandSourceStack) -> i32 {
    let entities = source.with_server(|s| s.scoreboard.read().get_tracked_players());
    if entities.is_empty() {
        source.send_success(|| translatable!("commands.scoreboard.players.list.empty"), false);
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.scoreboard.players.list.success",
                    entities.len() as i32,
                    format_list(entities.iter(), &default_separator(), |name| ScoreHolder::for_name_only(name).get_feedback_display_name())
                )
            },
            false,
        );
    }
    entities.len() as i32
}

fn list_tracked_player_scores(source: &CommandSourceStack, entity: &ScoreHolder) -> i32 {
    let scores = source.with_server(|s| {
        s.scoreboard.read().list_player_scores_for(entity)
            .into_iter()
            .map(|(objective, value)| (s.scoreboard.read().get_objective(&objective).expect("objective exists").get_formatted_display_name().clone(), value))
            .collect::<Vec<_>>()
    });
    if scores.is_empty() {
        source.send_success(|| translatable!("commands.scoreboard.players.list.entity.empty", entity.get_feedback_display_name()), false);
    } else {
        source.send_success(|| translatable!("commands.scoreboard.players.list.entity.success", entity.get_feedback_display_name(), scores.len() as i32), false);
        for (objective, value) in &scores {
            source.send_success(|| translatable!("commands.scoreboard.players.list.entity.entry", objective.clone(), *value), false);
        }
    }
    scores.len() as i32
}

fn clear_display_slot(source: &CommandSourceStack, slot: DisplaySlot) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_display_objective(slot).is_none()) {
        return Err(ERROR_DISPLAY_SLOT_ALREADY_EMPTY.create());
    }
    with_scoreboard(source, |scoreboard| scoreboard.set_display_objective(slot, None));
    source.send_success(|| translatable!("commands.scoreboard.objectives.display.cleared", slot.get_serialized_name()), true);
    Ok(0)
}

fn set_display_slot(source: &CommandSourceStack, slot: DisplaySlot, objective: &str) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_display_objective(slot).is_some_and(|o| o.get_name() == objective)) {
        return Err(ERROR_DISPLAY_SLOT_ALREADY_SET.create());
    }
    with_scoreboard(source, |scoreboard| scoreboard.set_display_objective(slot, Some(objective)));
    let display_name = source.with_server(|s| s.scoreboard.read().get_objective(objective).expect("objective exists").get_display_name().clone());
    source.send_success(|| translatable!("commands.scoreboard.objectives.display.set", slot.get_serialized_name(), display_name), true);
    Ok(0)
}

fn set_display_name(source: &CommandSourceStack, objective: &str, display_name: Component) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| *s.scoreboard.read().get_objective(objective).expect("objective exists").get_display_name() != display_name) {
        with_scoreboard(source, |scoreboard| scoreboard.set_objective_display_name(objective, display_name));
        let formatted = formatted_name(source, objective);
        source.send_success(|| translatable!("commands.scoreboard.objectives.modify.displayname", objective.to_owned(), formatted), true);
    }
    Ok(0)
}

fn set_display_auto_update(source: &CommandSourceStack, objective: &str, display_auto_update: bool) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_objective(objective).expect("objective exists").display_auto_update() != display_auto_update) {
        with_scoreboard(source, |scoreboard| scoreboard.set_objective_display_auto_update(objective, display_auto_update));
        let formatted = formatted_name(source, objective);
        let key = if display_auto_update {
            "commands.scoreboard.objectives.modify.displayAutoUpdate.enable"
        } else {
            "commands.scoreboard.objectives.modify.displayAutoUpdate.disable"
        };
        source.send_success(|| translatable!(key, objective.to_owned(), formatted), true);
    }
    Ok(0)
}

fn set_objective_format(source: &CommandSourceStack, objective: &str, number_format: Option<NumberFormat>) -> Result<i32, CommandSyntaxException> {
    let set = number_format.is_some();
    with_scoreboard(source, |scoreboard| scoreboard.set_objective_number_format(objective, number_format));
    let key = if set { "commands.scoreboard.objectives.modify.objectiveFormat.set" } else { "commands.scoreboard.objectives.modify.objectiveFormat.clear" };
    source.send_success(|| translatable!(key, objective.to_owned()), true);
    Ok(0)
}

fn set_render_type(source: &CommandSourceStack, objective: &str, render_type: RenderType) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_objective(objective).expect("objective exists").get_render_type() != render_type) {
        with_scoreboard(source, |scoreboard| scoreboard.set_objective_render_type(objective, render_type));
        let formatted = formatted_name(source, objective);
        source.send_success(|| translatable!("commands.scoreboard.objectives.modify.rendertype", formatted), true);
    }
    Ok(0)
}

fn remove_objective(source: &CommandSourceStack, objective: &str) -> Result<i32, CommandSyntaxException> {
    let formatted = formatted_name(source, objective);
    let remaining = with_scoreboard(source, |scoreboard| {
        scoreboard.remove_objective(objective);
        scoreboard.get_objectives().count() as i32
    });
    source.send_success(|| translatable!("commands.scoreboard.objectives.remove.success", formatted), true);
    Ok(remaining)
}

fn add_objective(source: &CommandSourceStack, name: &str, criteria: ObjectiveCriteria, display_name: Component) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_objective(name).is_some()) {
        return Err(ERROR_OBJECTIVE_ALREADY_EXISTS.create());
    }
    let count = with_scoreboard(source, |scoreboard| {
        let render_type = criteria.get_default_render_type();
        scoreboard.add_objective(name, criteria, display_name, render_type, false, None).expect("checked above");
        scoreboard.get_objectives().count() as i32
    });
    let formatted = formatted_name(source, name);
    source.send_success(|| translatable!("commands.scoreboard.objectives.add.success", formatted), true);
    Ok(count)
}

fn list_objectives(source: &CommandSourceStack) -> i32 {
    let objectives: Vec<Component> = source.with_server(|s| s.scoreboard.read().get_objectives().map(|o| o.get_formatted_display_name().clone()).collect());
    if objectives.is_empty() {
        source.send_success(|| translatable!("commands.scoreboard.objectives.list.empty"), false);
    } else {
        source.send_success(
            || translatable!("commands.scoreboard.objectives.list.success", objectives.len() as i32, format_list(objectives.iter(), &default_separator(), |o| o.clone())),
            false,
        );
    }
    objectives.len() as i32
}
