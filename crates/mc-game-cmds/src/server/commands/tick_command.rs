//! `TickCommand`.
use mc_brigadier::ArgumentBuilder;
use crate::commands::arguments::TimeArgument;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, literal};
use crate::util::java_format::format_fixed;
use crate::util::time_util::NANOSECONDS_PER_MILLISECOND;
use crate::{register_command, translatable};
use mc_brigadier::arguments::float_arg_range;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use std::sync::LazyLock;

const MAX_TICKRATE: f32 = 10000.0;
const DEFAULT_TICKRATE: &str = "20";

static ERROR_TICK_STEP_NOT_FROZEN: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.tick.step.fail"));
static ERROR_STEP_STOP_NOT_STEPPING: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.tick.step.stop.fail"));
static ERROR_SPRINT_STOP_NOT_SPRINTING: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.tick.sprint.stop.fail"));

register_command!("TickCommand", |ctx| {
    ctx.dispatcher.register(
        literal("tick")
            .requires(has_permission(LEVEL_ADMINS))
            .then(literal("query").executes(|c| Ok(tick_query(c.get_source()))))
            .then(literal("rate").then(
                argument("rate", float_arg_range(1.0, MAX_TICKRATE))
                    .suggests(|_, b| Ok(suggest([DEFAULT_TICKRATE], b)))
                    .executes(|c| Ok(set_ticking_rate(c.get_source(), *c.get_argument::<f32>("rate")))),
            ))
            .then(
                literal("step")
                    .executes(|c| step(c.get_source(), 1))
                    .then(literal("stop").executes(|c| stop_stepping(c.get_source())))
                    .then(
                        argument("time", TimeArgument::time_min(1))
                            .suggests(|_, b| Ok(suggest(["1t", "1s"], b)))
                            .executes(|c| step(c.get_source(), *c.get_argument::<i32>("time"))),
                    ),
            )
            .then(
                literal("sprint")
                    .then(literal("stop").executes(|c| stop_sprinting(c.get_source())))
                    .then(
                        argument("time", TimeArgument::time_min(1))
                            .suggests(|_, b| Ok(suggest(["60s", "1d", "3d"], b)))
                            .executes(|c| Ok(sprint(c.get_source(), *c.get_argument::<i32>("time")))),
                    ),
            )
            .then(literal("unfreeze").executes(|c| Ok(set_freeze(c.get_source(), false))))
            .then(literal("freeze").executes(|c| Ok(set_freeze(c.get_source(), true)))),
    );
});

/// `String.format(Locale.ROOT, "%.1f", (float)nanos / (float)TimeUtil.NANOSECONDS_PER_MILLISECOND)`.
fn nanos_to_millis_string(nanos: i64) -> String {
    format_fixed(f64::from(nanos as f32 / NANOSECONDS_PER_MILLISECOND as f32), 1)
}

fn set_ticking_rate(source: &CommandSourceStack, rate: f32) -> i32 {
    source.with_server(|server| server.with_tick_rate_manager(|manager, host| manager.set_tick_rate(rate, host)));
    let tick_rate_string = format_fixed(f64::from(rate), 1);
    source.send_success(|| translatable!("commands.tick.rate.success", tick_rate_string), true);
    rate as i32
}

fn tick_query(source: &CommandSourceStack) -> i32 {
    let (average, tick_rate, sprinting, frozen, nanos_per_tick, mut samples) = source.with_server(|server| {
        let manager = &server.tick_rate_manager;
        (
            server.get_average_tick_time_nanos(),
            manager.tickrate(),
            manager.is_sprinting(),
            manager.is_frozen(),
            manager.nanoseconds_per_tick(),
            server.get_tick_times_nanos().to_vec(),
        )
    });
    let busy_time = nanos_to_millis_string(average);
    let tick_rate_string = format_fixed(f64::from(tick_rate), 1);
    if sprinting {
        source.send_success(|| translatable!("commands.tick.status.sprinting"), false);
        source.send_success(|| translatable!("commands.tick.query.rate.sprinting", tick_rate_string, busy_time), false);
    } else {
        if frozen {
            source.send_success(|| translatable!("commands.tick.status.frozen"), false);
        } else if nanos_per_tick < average {
            source.send_success(|| translatable!("commands.tick.status.lagging"), false);
        } else {
            source.send_success(|| translatable!("commands.tick.status.running"), false);
        }
        let target = nanos_to_millis_string(nanos_per_tick);
        source.send_success(|| translatable!("commands.tick.query.rate.running", tick_rate_string, busy_time, target), false);
    }
    samples.sort_unstable();
    let len = samples.len();
    let p50 = nanos_to_millis_string(samples[len / 2]);
    let p95 = nanos_to_millis_string(samples[(len as f64 * 0.95) as usize]);
    let p99 = nanos_to_millis_string(samples[(len as f64 * 0.99) as usize]);
    source.send_success(|| translatable!("commands.tick.query.percentiles", p50, p95, p99, len as i32), false);
    tick_rate as i32
}

fn sprint(source: &CommandSourceStack, time: i32) -> i32 {
    let interrupted = source.with_server(|server| server.with_tick_rate_manager(|manager, host| manager.request_game_to_sprint(time, host)));
    if interrupted {
        source.send_success(|| translatable!("commands.tick.sprint.stop.success"), true);
    }
    source.send_success(|| translatable!("commands.tick.status.sprinting"), true);
    1
}

fn set_freeze(source: &CommandSourceStack, freeze: bool) -> i32 {
    source.with_server(|server| {
        server.with_tick_rate_manager(|manager, host| {
            if freeze {
                if manager.is_sprinting() {
                    manager.stop_sprinting(host);
                }
                if manager.is_stepping_forward() {
                    manager.stop_stepping(host);
                }
            }
            manager.set_frozen(freeze, host);
        })
    });
    if freeze {
        source.send_success(|| translatable!("commands.tick.status.frozen"), true);
    } else {
        source.send_success(|| translatable!("commands.tick.status.running"), true);
    }
    i32::from(freeze)
}

fn step(source: &CommandSourceStack, advance: i32) -> Result<i32, CommandSyntaxException> {
    if !source.with_server(|server| server.with_tick_rate_manager(|manager, host| manager.step_game_if_paused(advance, host))) {
        return Err(ERROR_TICK_STEP_NOT_FROZEN.create());
    }
    source.send_success(|| translatable!("commands.tick.step.success", advance), true);
    Ok(1)
}

fn stop_stepping(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if !source.with_server(|server| server.with_tick_rate_manager(|manager, host| manager.stop_stepping(host))) {
        return Err(ERROR_STEP_STOP_NOT_STEPPING.create());
    }
    source.send_success(|| translatable!("commands.tick.step.stop.success"), true);
    Ok(1)
}

fn stop_sprinting(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if !source.with_server(|server| server.with_tick_rate_manager(|manager, host| manager.stop_sprinting(host))) {
        return Err(ERROR_SPRINT_STOP_NOT_SPRINTING.create());
    }
    source.send_success(|| translatable!("commands.tick.sprint.stop.success"), true);
    Ok(1)
}
