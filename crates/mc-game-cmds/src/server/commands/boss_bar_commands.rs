//! `BossBarCommands`.
use mc_brigadier::arguments::{BoolArgumentType, IntegerArgumentType, integer_min};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, DynamicCommandExceptionType,
    LiteralArgumentBuilder, SimpleCommandExceptionType, SuggestionProvider,
};
use mc_core::ident::Identifier;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{ComponentArgument, EntityArgument, IdentifierArgument};
use mc_game_core::commands::component_utils::{ResolutionContext, resolve};
use mc_game_core::commands::message::{dynamic_translatable, simple_translatable};
use mc_game_core::commands::shared_suggestion_provider::suggest_resource;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::server::bossevents::{BossBarColor, BossBarOverlay, CustomBossEvent};
use mc_game_core::server::players::PlayerHandle;
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use mc_text::util::{default_separator, format_list};
use std::sync::{Arc, LazyLock};

type Ctx = CommandContext<CommandSourceStack>;

static ERROR_ALREADY_EXISTS: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.bossbar.create.failed"));
static ERROR_DOESNT_EXIST: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.bossbar.unknown"));
static ERROR_NO_PLAYER_CHANGE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.players.unchanged"));
static ERROR_NO_NAME_CHANGE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.name.unchanged"));
static ERROR_NO_COLOR_CHANGE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.color.unchanged"));
static ERROR_NO_STYLE_CHANGE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.style.unchanged"));
static ERROR_NO_VALUE_CHANGE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.value.unchanged"));
static ERROR_NO_MAX_CHANGE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.max.unchanged"));
static ERROR_ALREADY_HIDDEN: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.visibility.unchanged.hidden"));
static ERROR_ALREADY_VISIBLE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.bossbar.set.visibility.unchanged.visible"));

/// `BossBarCommands.SUGGEST_BOSS_BAR`.
pub fn suggest_boss_bar() -> SuggestionProvider<CommandSourceStack> {
    Arc::new(|context, builder| {
        let ids: Vec<Identifier> = context.get_source().with_server(|server| {
            server
                .custom_boss_events()
                .get_ids()
                .into_iter()
                .cloned()
                .collect()
        });
        Ok(suggest_resource(ids, builder))
    })
}

/// `BossBarCommands.getBossBar(context)`: the bar's id (argument `id`); unknown ids fail with `commands.bossbar.unknown`.
pub fn get_boss_bar(context: &Ctx) -> Result<Identifier, CommandSyntaxException> {
    let id = IdentifierArgument::get_id(context, "id");
    if context
        .get_source()
        .with_server(|server| server.custom_boss_events().get(&id).is_none())
    {
        return Err(ERROR_DOESNT_EXIST.create(id.to_string()));
    }
    Ok(id)
}

fn with_bar<R>(
    source: &CommandSourceStack,
    id: &Identifier,
    f: impl FnOnce(&mut CustomBossEvent) -> R,
) -> R {
    source.with_server(|server| {
        f(server
            .custom_boss_events()
            .get_mut(id)
            .expect("boss bar checked by getBossBar"))
    })
}

fn display_name(source: &CommandSourceStack, id: &Identifier) -> Component {
    with_bar(source, id, |bar| bar.get_display_name())
}

fn player_names(source: &CommandSourceStack, players: &[PlayerHandle]) -> Component {
    source.with_server(|server| {
        format_list(players.iter(), &default_separator(), |p| {
            host::player_display_name(server, p)
        })
    })
}

fn color_node(
    name: &'static str,
    color: BossBarColor,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    literal(name).executes(move |c| set_color(c.get_source(), &get_boss_bar(c)?, color))
}

fn style_node(
    name: &'static str,
    style: BossBarOverlay,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    literal(name).executes(move |c| set_style(c.get_source(), &get_boss_bar(c)?, style))
}

register_command!("BossBarCommands", |ctx| {
    ctx.dispatcher.register(
        literal("bossbar")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("add").then(argument("id", IdentifierArgument::id()).then(
                    argument("name", ComponentArgument::text_component()).executes(|c| {
                        create_bar(
                            c.get_source(),
                            IdentifierArgument::get_id(c, "id"),
                            resolved_name(c)?,
                        )
                    }),
                )),
            )
            .then(
                literal("remove").then(
                    argument("id", IdentifierArgument::id())
                        .suggests_provider(Some(suggest_boss_bar()))
                        .executes(|c| Ok(remove_bar(c.get_source(), &get_boss_bar(c)?))),
                ),
            )
            .then(literal("list").executes(|c| Ok(list_bars(c.get_source()))))
            .then(
                literal("set").then(
                    argument("id", IdentifierArgument::id())
                        .suggests_provider(Some(suggest_boss_bar()))
                        .then(literal("name").then(
                            argument("name", ComponentArgument::text_component()).executes(|c| {
                                set_name(c.get_source(), &get_boss_bar(c)?, resolved_name(c)?)
                            }),
                        ))
                        .then(
                            literal("color")
                                .then(color_node("pink", BossBarColor::Pink))
                                .then(color_node("blue", BossBarColor::Blue))
                                .then(color_node("red", BossBarColor::Red))
                                .then(color_node("green", BossBarColor::Green))
                                .then(color_node("yellow", BossBarColor::Yellow))
                                .then(color_node("purple", BossBarColor::Purple))
                                .then(color_node("white", BossBarColor::White)),
                        )
                        .then(
                            literal("style")
                                .then(style_node("progress", BossBarOverlay::Progress))
                                .then(style_node("notched_6", BossBarOverlay::Notched6))
                                .then(style_node("notched_10", BossBarOverlay::Notched10))
                                .then(style_node("notched_12", BossBarOverlay::Notched12))
                                .then(style_node("notched_20", BossBarOverlay::Notched20)),
                        )
                        .then(
                            literal("value").then(argument("value", integer_min(0)).executes(
                                |c| {
                                    set_value(
                                        c.get_source(),
                                        &get_boss_bar(c)?,
                                        IntegerArgumentType::get_integer(c, "value"),
                                    )
                                },
                            )),
                        )
                        .then(
                            literal("max").then(argument("max", integer_min(1)).executes(|c| {
                                set_max(
                                    c.get_source(),
                                    &get_boss_bar(c)?,
                                    IntegerArgumentType::get_integer(c, "max"),
                                )
                            })),
                        )
                        .then(literal("visible").then(
                            argument("visible", BoolArgumentType::bool()).executes(|c| {
                                set_visible(
                                    c.get_source(),
                                    &get_boss_bar(c)?,
                                    BoolArgumentType::get_bool(c, "visible"),
                                )
                            }),
                        ))
                        .then(
                            literal("players")
                                .executes(|c| {
                                    set_players(c.get_source(), &get_boss_bar(c)?, Vec::new())
                                })
                                .then(argument("targets", EntityArgument::players()).executes(
                                    |c| {
                                        set_players(
                                            c.get_source(),
                                            &get_boss_bar(c)?,
                                            EntityArgument::get_optional_players(c, "targets")?,
                                        )
                                    },
                                )),
                        ),
                ),
            )
            .then(
                literal("get").then(
                    argument("id", IdentifierArgument::id())
                        .suggests_provider(Some(suggest_boss_bar()))
                        .then(
                            literal("value")
                                .executes(|c| Ok(get_value(c.get_source(), &get_boss_bar(c)?))),
                        )
                        .then(
                            literal("max")
                                .executes(|c| Ok(get_max(c.get_source(), &get_boss_bar(c)?))),
                        )
                        .then(
                            literal("visible")
                                .executes(|c| Ok(get_visible(c.get_source(), &get_boss_bar(c)?))),
                        )
                        .then(
                            literal("players")
                                .executes(|c| Ok(get_players(c.get_source(), &get_boss_bar(c)?))),
                        ),
                ),
            ),
    );
});

/// `ComponentArgument.getResolvedComponent(context, "name")`.
fn resolved_name(c: &Ctx) -> Result<Component, CommandSyntaxException> {
    resolve(
        &ResolutionContext::create(c.get_source()),
        &ComponentArgument::get_raw_component(c, "name"),
    )
}

/// `ComponentUtils.resolve(builder().withSource(source).withEntityOverride(null).build(), name)`.
fn resolve_without_entity(
    source: &CommandSourceStack,
    name: &Component,
) -> Result<Component, CommandSyntaxException> {
    resolve(
        &ResolutionContext {
            default_scoreboard_entity: None,
            ..ResolutionContext::create(source)
        },
        name,
    )
}

fn get_value(source: &CommandSourceStack, id: &Identifier) -> i32 {
    let value = with_bar(source, id, |bar| bar.value());
    source.send_success(
        || {
            translatable!(
                "commands.bossbar.get.value",
                display_name(source, id),
                value
            )
        },
        true,
    );
    value
}

fn get_max(source: &CommandSourceStack, id: &Identifier) -> i32 {
    let max = with_bar(source, id, |bar| bar.max());
    source.send_success(
        || translatable!("commands.bossbar.get.max", display_name(source, id), max),
        true,
    );
    max
}

fn get_visible(source: &CommandSourceStack, id: &Identifier) -> i32 {
    if with_bar(source, id, |bar| bar.is_visible()) {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.get.visible.visible",
                    display_name(source, id)
                )
            },
            true,
        );
        1
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.get.visible.hidden",
                    display_name(source, id)
                )
            },
            true,
        );
        0
    }
}

fn get_players(source: &CommandSourceStack, id: &Identifier) -> i32 {
    let players = with_bar(source, id, |bar| bar.online_players());
    if players.is_empty() {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.get.players.none",
                    display_name(source, id)
                )
            },
            true,
        );
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.get.players.some",
                    display_name(source, id),
                    players.len() as i32,
                    player_names(source, &players)
                )
            },
            true,
        );
    }
    players.len() as i32
}

fn set_visible(
    source: &CommandSourceStack,
    id: &Identifier,
    visible: bool,
) -> Result<i32, CommandSyntaxException> {
    if with_bar(source, id, |bar| bar.is_visible()) == visible {
        return Err(if visible {
            ERROR_ALREADY_VISIBLE.create()
        } else {
            ERROR_ALREADY_HIDDEN.create()
        });
    }
    with_bar(source, id, |bar| bar.set_visible(visible));
    let key = if visible {
        "commands.bossbar.set.visible.success.visible"
    } else {
        "commands.bossbar.set.visible.success.hidden"
    };
    source.send_success(|| translatable!(key, display_name(source, id)), true);
    Ok(0)
}

fn set_value(
    source: &CommandSourceStack,
    id: &Identifier,
    value: i32,
) -> Result<i32, CommandSyntaxException> {
    if with_bar(source, id, |bar| bar.value()) == value {
        return Err(ERROR_NO_VALUE_CHANGE.create());
    }
    with_bar(source, id, |bar| bar.set_value(value));
    source.send_success(
        || {
            translatable!(
                "commands.bossbar.set.value.success",
                display_name(source, id),
                value
            )
        },
        true,
    );
    Ok(value)
}

fn set_max(
    source: &CommandSourceStack,
    id: &Identifier,
    value: i32,
) -> Result<i32, CommandSyntaxException> {
    if with_bar(source, id, |bar| bar.max()) == value {
        return Err(ERROR_NO_MAX_CHANGE.create());
    }
    with_bar(source, id, |bar| bar.set_max(value));
    source.send_success(
        || {
            translatable!(
                "commands.bossbar.set.max.success",
                display_name(source, id),
                value
            )
        },
        true,
    );
    Ok(value)
}

fn set_color(
    source: &CommandSourceStack,
    id: &Identifier,
    color: BossBarColor,
) -> Result<i32, CommandSyntaxException> {
    if with_bar(source, id, |bar| bar.get_color()) == color {
        return Err(ERROR_NO_COLOR_CHANGE.create());
    }
    with_bar(source, id, |bar| bar.set_color(color));
    source.send_success(
        || {
            translatable!(
                "commands.bossbar.set.color.success",
                display_name(source, id)
            )
        },
        true,
    );
    Ok(0)
}

fn set_style(
    source: &CommandSourceStack,
    id: &Identifier,
    style: BossBarOverlay,
) -> Result<i32, CommandSyntaxException> {
    if with_bar(source, id, |bar| bar.get_overlay()) == style {
        return Err(ERROR_NO_STYLE_CHANGE.create());
    }
    with_bar(source, id, |bar| bar.set_overlay(style));
    source.send_success(
        || {
            translatable!(
                "commands.bossbar.set.style.success",
                display_name(source, id)
            )
        },
        true,
    );
    Ok(0)
}

fn set_name(
    source: &CommandSourceStack,
    id: &Identifier,
    name: Component,
) -> Result<i32, CommandSyntaxException> {
    let replaced = resolve_without_entity(source, &name)?;
    if with_bar(source, id, |bar| bar.get_name() == &replaced) {
        return Err(ERROR_NO_NAME_CHANGE.create());
    }
    with_bar(source, id, |bar| bar.set_name(replaced));
    source.send_success(
        || {
            translatable!(
                "commands.bossbar.set.name.success",
                display_name(source, id)
            )
        },
        true,
    );
    Ok(0)
}

fn set_players(
    source: &CommandSourceStack,
    id: &Identifier,
    targets: Vec<EntityRef>,
) -> Result<i32, CommandSyntaxException> {
    let handles: Vec<PlayerHandle> = source.with_server(|server| {
        targets
            .iter()
            .filter_map(|t| host::player_handle(server, t))
            .collect()
    });
    if !with_bar(source, id, |bar| bar.set_players(&handles)) {
        return Err(ERROR_NO_PLAYER_CHANGE.create());
    }
    let online = with_bar(source, id, |bar| bar.online_players());
    if online.is_empty() {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.set.players.success.none",
                    display_name(source, id)
                )
            },
            true,
        );
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.set.players.success.some",
                    display_name(source, id),
                    handles.len() as i32,
                    player_names(source, &handles)
                )
            },
            true,
        );
    }
    Ok(online.len() as i32)
}

fn list_bars(source: &CommandSourceStack) -> i32 {
    let names: Vec<Component> = source.with_server(|server| {
        server
            .custom_boss_events()
            .get_events()
            .into_iter()
            .map(|e| e.get_display_name())
            .collect()
    });
    if names.is_empty() {
        source.send_success(|| translatable!("commands.bossbar.list.bars.none"), false);
    } else {
        source.send_success(
            || {
                translatable!(
                    "commands.bossbar.list.bars.some",
                    names.len() as i32,
                    format_list(names.iter().cloned(), &default_separator(), |n| n)
                )
            },
            false,
        );
    }
    names.len() as i32
}

fn create_bar(
    source: &CommandSourceStack,
    id: Identifier,
    name: Component,
) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|server| server.custom_boss_events().get(&id).is_some()) {
        return Err(ERROR_ALREADY_EXISTS.create(id.to_string()));
    }
    let name = resolve_without_entity(source, &name)?;
    let (display_name, count) = source.with_server(|server| {
        // `events.create(source.getLevel().getRandom(), ..)`: the level random supplies `createInsecureUUID`.
        let level = server
            .level_mut(source.get_level())
            .expect("source level exists");
        let uuid = mc_core::math::mth::create_insecure_uuid(&mut level.tick_state.random);
        let events = server
            .saved_data
            .compute_if_absent(&server.saved_types.custom_boss_events);
        let event = events.create_with_uuid(uuid, id.clone(), name);
        let display_name = event.get_display_name();
        (display_name, events.get_events().len() as i32)
    });
    source.send_success(
        || translatable!("commands.bossbar.create.success", display_name),
        true,
    );
    Ok(count)
}

fn remove_bar(source: &CommandSourceStack, id: &Identifier) -> i32 {
    let (display_name, count) = source.with_server(|server| {
        let events = server.custom_boss_events();
        let bar = events.get_mut(id).expect("boss bar checked by getBossBar");
        bar.remove_all_players();
        let display_name = bar.get_display_name();
        events.remove(id);
        (display_name, events.get_events().len() as i32)
    });
    source.send_success(
        || translatable!("commands.bossbar.remove.success", display_name),
        true,
    );
    count
}
