//! `ScheduleCommand`: `/schedule function <function|#tag> <time> [append|replace]` and `/schedule clear <function>`.
use crate::server::commands::function_command::{suggest_function, translation_arg};
use mc_brigadier::{
    ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType,
    SimpleCommandExceptionType,
};
use mc_core::ident::Identifier;
use mc_game_core::commands::arguments::function_argument::{FunctionArgument, FunctionOrTag};
use mc_game_core::commands::arguments::{IdentifierArgument, TimeArgument};
use mc_game_core::commands::message::{dynamic_translatable, simple_translatable};
use mc_game_core::commands::shared_suggestion_provider::suggest_resource;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::util::timer_queue::{FunctionCallback, FunctionTagCallback};
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

static ERROR_SAME_TICK: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.schedule.same_tick"));
static ERROR_CANT_REMOVE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.schedule.cleared.failure"));
static ERROR_MACRO: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.schedule.macro"));

register_command!("ScheduleCommand", |_ctx| {
    let schedule_with = |replace: bool| {
        move |c: &mc_brigadier::CommandContext<CommandSourceStack>| {
            schedule(
                c.get_source(),
                FunctionArgument::get_function_or_tag(c, "function")?,
                *c.get_argument::<i32>("time"),
                replace,
            )
        }
    };
    _ctx.dispatcher.register(
        literal("schedule")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("function").then(
                    argument("function", FunctionArgument::functions())
                        .suggests(suggest_function)
                        .then(
                            argument("time", TimeArgument::time())
                                .executes(schedule_with(true))
                                .then(literal("append").executes(schedule_with(false)))
                                .then(literal("replace").executes(schedule_with(true))),
                        ),
                ),
            )
            .then(
                literal("clear").then(
                    argument("function", IdentifierArgument::id())
                        .suggests(|c, builder| {
                            let ids: Vec<String> = c.get_source().with_server(|server| {
                                server
                                    .scheduled_events()
                                    .get_events_ids()
                                    .map(str::to_owned)
                                    .collect()
                            });
                            Ok(suggest_resource(
                                ids.iter().filter_map(|id| Identifier::parse(id).ok()),
                                builder,
                            ))
                        })
                        .executes(|c| {
                            remove(c.get_source(), &IdentifierArgument::get_id(c, "function"))
                        }),
                ),
            ),
    );
});

fn schedule(
    source: &CommandSourceStack,
    callback: (Identifier, FunctionOrTag),
    time: i32,
    replace: bool,
) -> Result<i32, CommandSyntaxException> {
    if time == 0 {
        return Err(ERROR_SAME_TICK.create());
    }
    let tick_time =
        source.with_server(|server| server.world_data.lock().game_time) + i64::from(time);
    let (callback_id, function_or_tag) = callback;
    match function_or_tag {
        FunctionOrTag::Function(function) => {
            if function.is_macro() {
                return Err(ERROR_MACRO.create());
            }
            let schedule_id = callback_id.to_string();
            source.with_server(|server| {
                let queue = server.scheduled_events();
                if replace {
                    queue.remove(&schedule_id);
                }
                queue.schedule(
                    &schedule_id,
                    tick_time,
                    Box::new(FunctionCallback {
                        function_id: callback_id.clone(),
                    }),
                );
            });
            source.send_success(
                || {
                    translatable!(
                        "commands.schedule.created.function",
                        translation_arg(&callback_id),
                        time,
                        tick_time
                    )
                },
                true,
            );
        }
        FunctionOrTag::Tag(_) => {
            let schedule_id = format!("#{callback_id}");
            source.with_server(|server| {
                let queue = server.scheduled_events();
                if replace {
                    queue.remove(&schedule_id);
                }
                queue.schedule(
                    &schedule_id,
                    tick_time,
                    Box::new(FunctionTagCallback {
                        tag_id: callback_id.clone(),
                    }),
                );
            });
            source.send_success(
                || {
                    translatable!(
                        "commands.schedule.created.tag",
                        translation_arg(&callback_id),
                        time,
                        tick_time
                    )
                },
                true,
            );
        }
    }
    Ok(tick_time.rem_euclid(i64::from(i32::MAX)) as i32)
}

fn remove(source: &CommandSourceStack, id: &Identifier) -> Result<i32, CommandSyntaxException> {
    let string_id = id.to_string();
    let count = source.with_server(|server| server.scheduled_events().remove(&string_id)) as i32;
    if count == 0 {
        return Err(ERROR_CANT_REMOVE.create(string_id));
    }
    source.send_success(
        || {
            translatable!(
                "commands.schedule.cleared.success",
                count,
                string_id.clone()
            )
        },
        true,
    );
    Ok(count)
}
