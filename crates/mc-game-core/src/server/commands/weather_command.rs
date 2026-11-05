//! `WeatherCommand`.
use crate::commands::arguments::TimeArgument;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_brigadier::ArgumentBuilder;
use crate::{register_command, translatable};

const DEFAULT_TIME: i32 = -1;
/// `ServerLevel.RAIN_DELAY` / `RAIN_DURATION` / `THUNDER_DURATION` (`UniformInt.of(min, max)`).
const RAIN_DELAY: (i32, i32) = (12000, 180000);
const RAIN_DURATION: (i32, i32) = (12000, 24000);
const THUNDER_DURATION: (i32, i32) = (3600, 15600);

register_command!("WeatherCommand", |ctx| {
    ctx.dispatcher.register(
        literal("weather")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("clear")
                    .executes(|c| Ok(set_clear(c.get_source(), DEFAULT_TIME)))
                    .then(argument("duration", TimeArgument::time_min(1)).executes(|c| Ok(set_clear(c.get_source(), *c.get_argument::<i32>("duration"))))),
            )
            .then(
                literal("rain")
                    .executes(|c| Ok(set_rain(c.get_source(), DEFAULT_TIME)))
                    .then(argument("duration", TimeArgument::time_min(1)).executes(|c| Ok(set_rain(c.get_source(), *c.get_argument::<i32>("duration"))))),
            )
            .then(
                literal("thunder")
                    .executes(|c| Ok(set_thunder(c.get_source(), DEFAULT_TIME)))
                    .then(argument("duration", TimeArgument::time_min(1)).executes(|c| Ok(set_thunder(c.get_source(), *c.get_argument::<i32>("duration"))))),
            ),
    );
});

/// `getDuration(source, input, defaultDistribution)`: samples the source level's random source.
fn get_duration(source: &CommandSourceStack, input: i32, default_distribution: (i32, i32)) -> i32 {
    if input == -1 { host::sample_uniform_int(source, default_distribution.0, default_distribution.1) } else { input }
}

fn set_clear(source: &CommandSourceStack, duration: i32) -> i32 {
    let clear = get_duration(source, duration, RAIN_DELAY);
    source.with_server(|server| server.set_weather_parameters(clear, 0, false, false));
    source.send_success(|| translatable!("commands.weather.set.clear"), true);
    duration
}

fn set_rain(source: &CommandSourceStack, duration: i32) -> i32 {
    let rain = get_duration(source, duration, RAIN_DURATION);
    source.with_server(|server| server.set_weather_parameters(0, rain, true, false));
    source.send_success(|| translatable!("commands.weather.set.rain"), true);
    duration
}

fn set_thunder(source: &CommandSourceStack, duration: i32) -> i32 {
    let thunder = get_duration(source, duration, THUNDER_DURATION);
    source.with_server(|server| server.set_weather_parameters(0, thunder, true, true));
    source.send_success(|| translatable!("commands.weather.set.thunder"), true);
    duration
}
