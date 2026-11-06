//! `WorldBorderCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::arguments::{double_arg_range, float_arg_min, integer_min};
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_game_core::commands::arguments::TimeArgument;
use mc_game_core::commands::arguments::coordinates::Vec2Argument;
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::util::java_format::format_fixed;
use mc_game_core::world::level::saveddata::world_border::WorldBorder;
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

/// `5.999997E7F` as a double (the `float` constant widened).
const MAX_SIZE: f64 = 5.999997E7_f32 as f64;
const MAX_CENTER_COORDINATE: f64 = 2.9999984E7;

static ERROR_SAME_CENTER: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.center.failed"));
static ERROR_SAME_SIZE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.set.failed.nochange"));
static ERROR_TOO_SMALL: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.set.failed.small"));
static ERROR_TOO_BIG: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| {
    SimpleCommandExceptionType::new(message(translatable!(
        "commands.worldborder.set.failed.big",
        5.999997E7_f32
    )))
});
static ERROR_TOO_FAR_OUT: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| {
    SimpleCommandExceptionType::new(message(translatable!(
        "commands.worldborder.set.failed.far",
        MAX_CENTER_COORDINATE
    )))
});
static ERROR_SAME_WARNING_TIME: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.warning.time.failed"));
static ERROR_SAME_WARNING_DISTANCE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.warning.distance.failed"));
static ERROR_SAME_DAMAGE_BUFFER: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.damage.buffer.failed"));
static ERROR_SAME_DAMAGE_AMOUNT: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.worldborder.damage.amount.failed"));

register_command!("WorldBorderCommand", |ctx| {
    ctx.dispatcher.register(
        literal("worldborder")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("add").then(
                    argument("distance", double_arg_range(-MAX_SIZE, MAX_SIZE))
                        .executes(|c| {
                            set_size(
                                c.get_source(),
                                with_border(c.get_source(), |b, _| b.get_size())?
                                    + *c.get_argument::<f64>("distance"),
                                0,
                            )
                        })
                        .then(argument("time", TimeArgument::time_min(0)).executes(|c| {
                            let (size, lerp_time) = with_border(c.get_source(), |b, _| {
                                (b.get_size(), b.get_lerp_time())
                            })?;
                            set_size(
                                c.get_source(),
                                size + *c.get_argument::<f64>("distance"),
                                lerp_time + i64::from(*c.get_argument::<i32>("time")),
                            )
                        })),
                ),
            )
            .then(
                literal("set").then(
                    argument("distance", double_arg_range(-MAX_SIZE, MAX_SIZE))
                        .executes(|c| {
                            set_size(c.get_source(), *c.get_argument::<f64>("distance"), 0)
                        })
                        .then(argument("time", TimeArgument::time_min(0)).executes(|c| {
                            set_size(
                                c.get_source(),
                                *c.get_argument::<f64>("distance"),
                                i64::from(*c.get_argument::<i32>("time")),
                            )
                        })),
                ),
            )
            .then(
                literal("center")
                    .then(argument("pos", Vec2Argument::vec2()).executes(|c| {
                        set_center(c.get_source(), Vec2Argument::get_vec2(c, "pos"))
                    })),
            )
            .then(
                literal("damage")
                    .then(literal("amount").then(
                        argument("damagePerBlock", float_arg_min(0.0)).executes(|c| {
                            set_damage_amount(
                                c.get_source(),
                                *c.get_argument::<f32>("damagePerBlock"),
                            )
                        }),
                    ))
                    .then(literal("buffer").then(
                        argument("distance", float_arg_min(0.0)).executes(|c| {
                            set_damage_buffer(c.get_source(), *c.get_argument::<f32>("distance"))
                        }),
                    )),
            )
            .then(literal("get").executes(|c| get_size(c.get_source())))
            .then(
                literal("warning")
                    .then(
                        literal("distance").then(argument("distance", integer_min(0)).executes(
                            |c| {
                                set_warning_distance(
                                    c.get_source(),
                                    *c.get_argument::<i32>("distance"),
                                )
                            },
                        )),
                    )
                    .then(literal("time").then(
                        argument("time", TimeArgument::time_min(0)).executes(|c| {
                            set_warning_time(c.get_source(), *c.get_argument::<i32>("time"))
                        }),
                    )),
            ),
    );
});

/// `source.getLevel().getWorldBorder()` (initial settings applied) plus the level's game time; the queued
/// `BorderChangeListener` broadcasts are flushed right away like vanilla's immediate listener calls.
fn with_border<R>(
    source: &CommandSourceStack,
    f: impl FnOnce(&mut WorldBorder, i64) -> R,
) -> Result<R, CommandSyntaxException> {
    Ok(source.with_server(|server| {
        let level = server
            .level_mut(source.get_level())
            .expect("a command source runs in a loaded level");
        let game_time = level.game_time();
        let result = f(level.get_world_border(), game_time);
        server.flush_world_border_events();
        result
    }))
}

fn format_ticks_to_seconds(ticks: i64) -> String {
    format_fixed(ticks as f64 / 20.0, 2)
}

fn set_damage_buffer(
    source: &CommandSourceStack,
    distance: f32,
) -> Result<i32, CommandSyntaxException> {
    let distance_d = f64::from(distance);
    if with_border(source, |b, _| b.get_safe_zone())? == distance_d {
        return Err(ERROR_SAME_DAMAGE_BUFFER.create());
    }
    with_border(source, |b, _| b.set_safe_zone(distance_d))?;
    source.send_success(
        || {
            translatable!(
                "commands.worldborder.damage.buffer.success",
                format_fixed(distance_d, 2)
            )
        },
        true,
    );
    Ok(distance as i32)
}

fn set_damage_amount(
    source: &CommandSourceStack,
    damage_per_block: f32,
) -> Result<i32, CommandSyntaxException> {
    let damage = f64::from(damage_per_block);
    if with_border(source, |b, _| b.get_damage_per_block())? == damage {
        return Err(ERROR_SAME_DAMAGE_AMOUNT.create());
    }
    with_border(source, |b, _| b.set_damage_per_block(damage))?;
    source.send_success(
        || {
            translatable!(
                "commands.worldborder.damage.amount.success",
                format_fixed(damage, 2)
            )
        },
        true,
    );
    Ok(damage_per_block as i32)
}

fn set_warning_time(
    source: &CommandSourceStack,
    ticks: i32,
) -> Result<i32, CommandSyntaxException> {
    if with_border(source, |b, _| b.get_warning_time())? == ticks {
        return Err(ERROR_SAME_WARNING_TIME.create());
    }
    with_border(source, |b, _| b.set_warning_time(ticks))?;
    source.send_success(
        || {
            translatable!(
                "commands.worldborder.warning.time.success",
                format_ticks_to_seconds(i64::from(ticks))
            )
        },
        true,
    );
    Ok(ticks)
}

fn set_warning_distance(
    source: &CommandSourceStack,
    distance: i32,
) -> Result<i32, CommandSyntaxException> {
    if with_border(source, |b, _| b.get_warning_blocks())? == distance {
        return Err(ERROR_SAME_WARNING_DISTANCE.create());
    }
    with_border(source, |b, _| b.set_warning_blocks(distance))?;
    source.send_success(
        || translatable!("commands.worldborder.warning.distance.success", distance),
        true,
    );
    Ok(distance)
}

fn get_size(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    let size = with_border(source, |b, _| b.get_size())?;
    source.send_success(
        || translatable!("commands.worldborder.get", format_fixed(size, 0)),
        false,
    );
    Ok(mc_core::math::mth::floor_f64(size + 0.5))
}

fn set_center(
    source: &CommandSourceStack,
    center: mc_core::phys::Vec2,
) -> Result<i32, CommandSyntaxException> {
    let (x, z) = (f64::from(center.x), f64::from(center.y));
    let (current_x, current_z) = with_border(source, |b, _| (b.get_center_x(), b.get_center_z()))?;
    if current_x == x && current_z == z {
        return Err(ERROR_SAME_CENTER.create());
    }
    if x.abs() > MAX_CENTER_COORDINATE || z.abs() > MAX_CENTER_COORDINATE {
        return Err(ERROR_TOO_FAR_OUT.create());
    }
    with_border(source, |b, _| b.set_center(x, z))?;
    source.send_success(
        || {
            translatable!(
                "commands.worldborder.center.success",
                format_fixed(x, 2),
                format_fixed(z, 2)
            )
        },
        true,
    );
    Ok(0)
}

fn set_size(
    source: &CommandSourceStack,
    distance: f64,
    ticks: i64,
) -> Result<i32, CommandSyntaxException> {
    let current = with_border(source, |b, _| b.get_size())?;
    if current == distance {
        return Err(ERROR_SAME_SIZE.create());
    }
    if distance < 1.0 {
        return Err(ERROR_TOO_SMALL.create());
    }
    if distance > MAX_SIZE {
        return Err(ERROR_TOO_BIG.create());
    }
    let formatted = format_fixed(distance, 1);
    if ticks > 0 {
        with_border(source, |b, game_time| {
            b.lerp_size_between(current, distance, ticks, game_time)
        })?;
        let key = if distance > current {
            "commands.worldborder.set.grow"
        } else {
            "commands.worldborder.set.shrink"
        };
        source.send_success(
            || translatable!(key, formatted, format_ticks_to_seconds(ticks)),
            true,
        );
    } else {
        with_border(source, |b, _| b.set_size(distance))?;
        source.send_success(
            || translatable!("commands.worldborder.set.immediate", formatted),
            true,
        );
    }
    Ok((distance - current) as i32)
}
