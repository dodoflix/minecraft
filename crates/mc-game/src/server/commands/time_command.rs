//! `TimeCommand`.
use crate::commands::arguments::resource_arguments::ResourceArgument;
use crate::commands::arguments::{IdentifierArgument, TimeArgument};
use crate::commands::message::message;
use crate::commands::shared_suggestion_provider::suggest_resource;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use crate::world::clock::MoveResult;
use crate::world::clock::ClockInstance;
use crate::{register_command, translatable};
use mc_brigadier::arguments::float_arg_range;
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType};
use mc_core::ident::Identifier;
use std::sync::{Arc, LazyLock};

type ClockGetter = Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> Result<Identifier, CommandSyntaxException> + Send + Sync>;

static ERROR_NO_DEFAULT_CLOCK: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|dimension: String| message(translatable!("commands.time.no_default_clock", dimension))));

fn two(key: &'static str, swap: bool) -> Dynamic2CommandExceptionType<String, String> {
    Dynamic2CommandExceptionType::new(move |a: String, b: String| {
        message(if swap { translatable!(key, b, a) } else { translatable!(key, a, b) })
    })
}

// `(clock, timeMarker)` -> `(timeMarker, clock)`; `(clock, timeline)` -> `(timeline, clock)`; the rest keep their order.
static ERROR_NO_TIME_MARKER_FOUND: LazyLock<Dynamic2CommandExceptionType<String, String>> = LazyLock::new(|| two("commands.time.no_time_marker_found", true));
static ERROR_WRONG_TIMELINE_FOR_CLOCK: LazyLock<Dynamic2CommandExceptionType<String, String>> = LazyLock::new(|| two("commands.time.wrong_timeline_for_clock", true));
static ERROR_ALREADY_AT_TIME_MARKER: LazyLock<Dynamic2CommandExceptionType<String, String>> = LazyLock::new(|| two("commands.time.set.already_at_time_marker", false));
static ERROR_ALREADY_AT_TIME: LazyLock<Dynamic2CommandExceptionType<String, i32>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|clock: String, time: i32| message(translatable!("commands.time.set.already_at_time", clock, time)))
});
static ERROR_ALREADY_PAUSED: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|clock: String| message(translatable!("commands.time.pause.already_paused", clock))));
static ERROR_ALREADY_RUNNING: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|clock: String| message(translatable!("commands.time.pause.already_running", clock))));
static ERROR_ALREADY_SAME_RATE: LazyLock<Dynamic2CommandExceptionType<String, f32>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|clock: String, rate: f32| message(translatable!("commands.time.rate.already_same", clock, rate)))
});

register_command!("TimeCommand", |ctx| {
    let default_clock: ClockGetter = Arc::new(|c| get_default_clock(c.get_source()));
    let base = || literal("time").requires(has_permission(LEVEL_GAMEMASTERS));
    ctx.dispatcher.register(add_clock_nodes(ctx.build_context, base(), default_clock.clone()));
    // vanilla keeps adding to the same builder: the second registration merges into the first one's literal.
    let base = add_clock_nodes(ctx.build_context, base(), default_clock);
    let clock_argument = argument("clock", ResourceArgument::resource(ctx.build_context, "world_clock"));
    let of_clock: ClockGetter = Arc::new(|c| ResourceArgument::get_resource(c, "clock", "world_clock").map(|k| k.id));
    ctx.dispatcher.register(
        base.then(literal("query").then(literal("gametime").executes(|c| Ok(query_game_time(c.get_source())))))
            .then(literal("of").then(add_clock_nodes(ctx.build_context, clock_argument, of_clock))),
    );
});

fn add_clock_nodes<A: ArgumentBuilder<CommandSourceStack>>(context: &crate::commands::CommandBuildContext, node: A, clock_getter: ClockGetter) -> A {
    let g = clock_getter.clone();
    let set = literal("set")
        .then(argument("time", TimeArgument::time()).executes({
            let g = g.clone();
            move |c| set_total_ticks(c.get_source(), &g(c)?, *c.get_argument::<i32>("time"))
        }))
        .then(
            argument("timemarker", IdentifierArgument::id())
                .suggests({
                    let g = g.clone();
                    move |c, b| Ok(suggest_time_markers(c.get_source(), b, &g(c)?))
                })
                .executes({
                    let g = g.clone();
                    move |c| set_time_to_time_marker(c.get_source(), &g(c)?, IdentifierArgument::get_id(c, "timemarker"))
                }),
        );
    let add = literal("add").then(argument("time", TimeArgument::time_min(i32::MIN)).executes({
        let g = g.clone();
        move |c| Ok(add_time(c.get_source(), &g(c)?, *c.get_argument::<i32>("time")))
    }));
    let pause = literal("pause").executes({
        let g = g.clone();
        move |c| set_paused(c.get_source(), &g(c)?, true)
    });
    let resume = literal("resume").executes({
        let g = g.clone();
        move |c| set_paused(c.get_source(), &g(c)?, false)
    });
    let rate = literal("rate").then(argument("rate", float_arg_range(1.0E-5, 1000.0)).executes({
        let g = g.clone();
        move |c| set_rate(c.get_source(), &g(c)?, *c.get_argument::<f32>("rate"))
    }));
    let query = literal("query")
        .then(literal("time").executes({
            let g = g.clone();
            move |c| Ok(query_time(c.get_source(), &g(c)?))
        }))
        .then(
            argument("timeline", ResourceArgument::resource(context, "timeline"))
                .suggests({
                    let g = g.clone();
                    move |c, b| Ok(suggest_timelines(c.get_source(), b, &g(c)?))
                })
                .executes({
                    let g = g.clone();
                    move |c| query_timeline_ticks(c.get_source(), &g(c)?, ResourceArgument::get_resource(c, "timeline", "timeline")?.id)
                })
                .then(literal("repetition").executes({
                    let g = g.clone();
                    move |c| query_timeline_repetitions(c.get_source(), &g(c)?, ResourceArgument::get_resource(c, "timeline", "timeline")?.id)
                })),
        );
    node.then(set).then(add).then(pause).then(resume).then(rate).then(query)
}

fn suggest_time_markers(source: &CommandSourceStack, builder: mc_brigadier::SuggestionsBuilder, clock: &Identifier) -> mc_brigadier::Suggestions {
    let markers: Vec<Identifier> = source.with_server(|server| server.clock_manager().command_time_markers_for_clock(clock).cloned().collect());
    suggest_resource(markers, builder)
}

fn suggest_timelines(source: &CommandSourceStack, builder: mc_brigadier::SuggestionsBuilder, clock: &Identifier) -> mc_brigadier::Suggestions {
    suggest_resource(source.with_server(|server| host::timelines_of_clock(server, clock)), builder)
}

fn query_game_time(source: &CommandSourceStack) -> i32 {
    let game_time = source.with_server(|server| server.world_data.lock().game_time);
    source.send_success(|| translatable!("commands.time.query.gametime", game_time), false);
    wrap_time(game_time)
}

fn total_ticks(source: &CommandSourceStack, clock: &Identifier) -> i64 {
    source.with_server(|server| server.clock_manager().get_instance(clock).total_ticks())
}

fn query_time(source: &CommandSourceStack, clock: &Identifier) -> i32 {
    let total_ticks = total_ticks(source, clock);
    source.send_success(|| translatable!("commands.time.query.absolute", clock.to_string(), total_ticks), false);
    wrap_time(total_ticks)
}

/// `(period count, current ticks)` of `timeline` for `clock`, or the wrong-timeline error.
fn timeline_ticks(source: &CommandSourceStack, clock: &Identifier, timeline: &Identifier) -> Result<(i64, i64, i64), CommandSyntaxException> {
    let Some((timeline_clock, period)) = source.with_server(|server| host::timeline(server, timeline)) else {
        return Err(ERROR_WRONG_TIMELINE_FOR_CLOCK.create(clock.to_string(), timeline.to_string()));
    };
    if &timeline_clock != clock {
        return Err(ERROR_WRONG_TIMELINE_FOR_CLOCK.create(clock.to_string(), timeline.to_string()));
    }
    let total = total_ticks(source, clock);
    let (current, repetitions) = match period {
        None => (total, 0),
        Some(p) => (total % i64::from(p), (total / i64::from(p)) as i32 as i64),
    };
    Ok((total, current, repetitions))
}

fn query_timeline_ticks(source: &CommandSourceStack, clock: &Identifier, timeline: Identifier) -> Result<i32, CommandSyntaxException> {
    let (_, current, _) = timeline_ticks(source, clock, &timeline)?;
    source.send_success(|| translatable!("commands.time.query.timeline", timeline.to_string(), current), false);
    Ok(wrap_time(current))
}

fn query_timeline_repetitions(source: &CommandSourceStack, clock: &Identifier, timeline: Identifier) -> Result<i32, CommandSyntaxException> {
    let (_, _, repetitions) = timeline_ticks(source, clock, &timeline)?;
    source.send_success(|| translatable!("commands.time.query.timeline.repetitions", timeline.to_string(), repetitions), false);
    Ok(wrap_time(repetitions))
}

fn set_total_ticks(source: &CommandSourceStack, clock: &Identifier, total_ticks_value: i32) -> Result<i32, CommandSyntaxException> {
    if total_ticks(source, clock) == i64::from(total_ticks_value) {
        return Err(ERROR_ALREADY_AT_TIME.create(clock.to_string(), total_ticks_value));
    }
    source.with_server(|server| server.with_clock_manager(|clocks, host| clocks.set_total_ticks(host, clock, i64::from(total_ticks_value))));
    source.send_success(|| translatable!("commands.time.set.absolute", clock.to_string(), total_ticks_value), true);
    Ok(total_ticks_value)
}

fn add_time(source: &CommandSourceStack, clock: &Identifier, time: i32) -> i32 {
    source.with_server(|server| server.with_clock_manager(|clocks, host| clocks.add_ticks(host, clock, time)));
    let total = total_ticks(source, clock);
    source.send_success(|| translatable!("commands.time.set.absolute", clock.to_string(), total), true);
    wrap_time(total)
}

fn set_time_to_time_marker(source: &CommandSourceStack, clock: &Identifier, marker: Identifier) -> Result<i32, CommandSyntaxException> {
    let result = source.with_server(|server| server.with_clock_manager(|clocks, host| clocks.move_to_time_marker(host, clock, &marker)));
    let (clock_name, marker_name) = (clock.to_string(), marker.to_string());
    match result {
        MoveResult::NoTimeMarkerFound => return Err(ERROR_NO_TIME_MARKER_FOUND.create(clock_name, marker_name)),
        MoveResult::NotMoved => return Err(ERROR_ALREADY_AT_TIME_MARKER.create(clock_name, marker_name)),
        MoveResult::Moved => source.send_success(|| translatable!("commands.time.set.time_marker", clock_name.clone(), marker_name.clone()), true),
    }
    Ok(wrap_time(total_ticks(source, clock)))
}

fn set_paused(source: &CommandSourceStack, clock: &Identifier, paused: bool) -> Result<i32, CommandSyntaxException> {
    let clock_name = clock.to_string();
    if source.with_server(|server| server.clock_manager().get_instance(clock).is_paused()) == paused {
        return Err(if paused { ERROR_ALREADY_PAUSED.create(clock_name) } else { ERROR_ALREADY_RUNNING.create(clock_name) });
    }
    source.with_server(|server| server.with_clock_manager(|clocks, host| clocks.set_paused(host, clock, paused)));
    source.send_success(|| translatable!(if paused { "commands.time.pause" } else { "commands.time.resume" }, clock_name.clone()), true);
    Ok(1)
}

fn set_rate(source: &CommandSourceStack, clock: &Identifier, rate: f32) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|server| server.clock_manager().get_instance(clock).rate()) == rate {
        return Err(ERROR_ALREADY_SAME_RATE.create(clock.to_string(), rate));
    }
    source.with_server(|server| server.with_clock_manager(|clocks, host| clocks.set_rate(host, clock, rate)));
    source.send_success(|| translatable!("commands.time.rate", clock.to_string(), rate), true);
    Ok(1)
}

/// `Math.toIntExact(ticks % 2147483647L)`.
fn wrap_time(ticks: i64) -> i32 {
    (ticks % 2147483647) as i32
}

fn get_default_clock(source: &CommandSourceStack) -> Result<Identifier, CommandSyntaxException> {
    let (clock, name) = source.with_server(|server| (host::default_clock(server, source.get_level()), host::dimension_type_name(server, source.get_level())));
    clock.ok_or_else(|| ERROR_NO_DEFAULT_CLOCK.create(name))
}
