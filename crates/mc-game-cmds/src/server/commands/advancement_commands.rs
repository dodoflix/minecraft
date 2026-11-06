//! `AdvancementCommands`.
use super::command_response_tracker::{CommandResponseTracker, ElementType, display_name};
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    Dynamic3CommandExceptionType,
};
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::resource_arguments::ResourceKeyArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::host::{self, AdvancementRef};
use mc_game_core::commands::message::message;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_CRITERION_NOT_FOUND: LazyLock<Dynamic2CommandExceptionType<Component, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|name: Component, criterion: String| {
            message(translatable!(
                "commands.advancement.criterionNotFound",
                name,
                criterion
            ))
        })
    });

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Grant,
    Revoke,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Only,
    Through,
    From,
    Until,
}

impl Mode {
    fn parents(self) -> bool {
        matches!(self, Self::Through | Self::Until)
    }

    fn children(self) -> bool {
        matches!(self, Self::Through | Self::From)
    }
}

type Ctx = CommandContext<CommandSourceStack>;

register_command!("AdvancementCommands", |ctx| {
    fn advancement_suggestions(
        c: &Ctx,
        p: mc_brigadier::SuggestionsBuilder,
    ) -> Result<mc_brigadier::Suggestions, CommandSyntaxException> {
        Ok(suggest(
            ResourceKeyArgument::get_advancement(c, "advancement")?.criteria,
            p,
        ))
    }

    fn mode_branch(
        action: Action,
        name: &'static str,
        mode: Mode,
    ) -> mc_brigadier::LiteralArgumentBuilder<CommandSourceStack> {
        literal(name).then(
            argument("advancement", ResourceKeyArgument::key("advancement")).executes(move |c| {
                let players = EntityArgument::get_players(c, "targets")?;
                let advancements = get_advancements(
                    c,
                    ResourceKeyArgument::get_advancement(c, "advancement")?,
                    mode,
                );
                perform(c.get_source(), &players, action, &advancements, true)
            }),
        )
    }

    fn action_branch(
        action: Action,
        name: &'static str,
    ) -> mc_brigadier::LiteralArgumentBuilder<CommandSourceStack> {
        let only = literal("only").then(
            argument("advancement", ResourceKeyArgument::key("advancement"))
                .executes(move |c| {
                    let players = EntityArgument::get_players(c, "targets")?;
                    let advancements = get_advancements(
                        c,
                        ResourceKeyArgument::get_advancement(c, "advancement")?,
                        Mode::Only,
                    );
                    perform(c.get_source(), &players, action, &advancements, true)
                })
                .then(
                    argument("criterion", StringArgumentType::greedy_string())
                        .suggests(advancement_suggestions)
                        .executes(move |c| {
                            perform_criterion(
                                c.get_source(),
                                &EntityArgument::get_players(c, "targets")?,
                                action,
                                &ResourceKeyArgument::get_advancement(c, "advancement")?,
                                StringArgumentType::get_string(c, "criterion"),
                            )
                        }),
                ),
        );
        // `grant everything` hides the toasts (`showAdvancements = false`), `revoke everything` does not.
        let show_everything = action == Action::Revoke;
        literal(name).then(
            argument("targets", EntityArgument::players())
                .then(only)
                .then(mode_branch(action, "from", Mode::From))
                .then(mode_branch(action, "until", Mode::Until))
                .then(mode_branch(action, "through", Mode::Through))
                .then(literal("everything").executes(move |c| {
                    let players = EntityArgument::get_players(c, "targets")?;
                    let all = c
                        .get_source()
                        .with_server(|server| host::advancement_all(server));
                    perform(c.get_source(), &players, action, &all, show_everything)
                })),
        )
    }

    ctx.dispatcher.register(
        literal("advancement")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(action_branch(Action::Grant, "grant"))
            .then(action_branch(Action::Revoke, "revoke")),
    );
});

impl Action {
    fn key(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Revoke => "revoke",
        }
    }

    /// `Action.perform(player, advancements, showAdvancements)`.
    fn perform_all(
        self,
        source: &CommandSourceStack,
        player: &EntityRef,
        advancements: &[AdvancementRef],
        show: bool,
    ) -> i32 {
        source.with_server(|server| {
            if !show {
                host::player_advancements_flush_dirty(server, player, true);
            }
            let mut count = 0;
            for advancement in advancements {
                if self.perform(server, player, advancement) {
                    count += 1;
                }
            }
            if !show {
                host::player_advancements_flush_dirty(server, player, false);
            }
            count
        })
    }

    fn perform(
        self,
        server: &mut mc_game_core::server::MinecraftServer,
        player: &EntityRef,
        advancement: &AdvancementRef,
    ) -> bool {
        let progress = host::player_advancement_progress(server, player, &advancement.id);
        match self {
            Self::Grant => {
                if progress.done {
                    return false;
                }
                for criterion in &progress.remaining {
                    host::player_advancement_award(server, player, &advancement.id, criterion);
                }
                true
            }
            Self::Revoke => {
                if !progress.has_progress {
                    return false;
                }
                for criterion in &progress.completed {
                    host::player_advancement_revoke(server, player, &advancement.id, criterion);
                }
                true
            }
        }
    }

    fn perform_criterion(
        self,
        source: &CommandSourceStack,
        player: &EntityRef,
        advancement: &AdvancementRef,
        criterion: &str,
    ) -> bool {
        source.with_server(|server| match self {
            Self::Grant => {
                host::player_advancement_award(server, player, &advancement.id, criterion)
            }
            Self::Revoke => {
                host::player_advancement_revoke(server, player, &advancement.id, criterion)
            }
        })
    }
}

fn perform(
    source: &CommandSourceStack,
    players: &[EntityRef],
    action: Action,
    advancements: &[AdvancementRef],
    show_advancements: bool,
) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in players {
        tracker.track(
            player,
            action.perform_all(source, player, advancements, show_advancements),
        );
    }
    let key = action.key();
    let advancement_count = advancements.len() as i32;
    if let [advancement] = advancements {
        let name = &advancement.name;
        if tracker.total_value() == 0 {
            return Err(tracker.dispatch(
                ElementType::Any,
                |player, _| {
                    Dynamic2CommandExceptionType::new(move |a: Component, p: Component| {
                        message(translatable!(
                            format!("commands.advancement.{key}.one.to.one.failure"),
                            a,
                            p
                        ))
                    })
                    .create(name.clone(), display_name(source, player))
                },
                |player_count, _| {
                    Dynamic2CommandExceptionType::new(move |a: Component, n: i32| {
                        message(translatable!(
                            format!("commands.advancement.{key}.one.to.many.failure"),
                            a,
                            n
                        ))
                    })
                    .create(name.clone(), player_count)
                },
            ));
        }
        return tracker.send_feedback(
            source,
            true,
            ElementType::NonZero,
            None::<fn() -> CommandSyntaxException>,
            |player, _| {
                translatable!(
                    format!("commands.advancement.{key}.one.to.one.success"),
                    name.clone(),
                    display_name(source, player)
                )
            },
            |player_count, _| {
                translatable!(
                    format!("commands.advancement.{key}.one.to.many.success"),
                    name.clone(),
                    player_count
                )
            },
        );
    }
    if tracker.total_value() == 0 {
        return Err(tracker.dispatch(
            ElementType::Any,
            |player, _| {
                Dynamic2CommandExceptionType::new(move |n: i32, p: Component| {
                    message(translatable!(
                        format!("commands.advancement.{key}.many.to.one.failure"),
                        n,
                        p
                    ))
                })
                .create(advancement_count, display_name(source, player))
            },
            |player_count, _| {
                Dynamic2CommandExceptionType::new(move |n: i32, p: i32| {
                    message(translatable!(
                        format!("commands.advancement.{key}.many.to.many.failure"),
                        n,
                        p
                    ))
                })
                .create(advancement_count, player_count)
            },
        ));
    }
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |player, _| {
            translatable!(
                format!("commands.advancement.{key}.many.to.one.success"),
                advancement_count,
                display_name(source, player)
            )
        },
        |player_count, _| {
            translatable!(
                format!("commands.advancement.{key}.many.to.many.success"),
                advancement_count,
                player_count
            )
        },
    )
}

fn perform_criterion(
    source: &CommandSourceStack,
    players: &[EntityRef],
    action: Action,
    advancement: &AdvancementRef,
    criterion: &str,
) -> Result<i32, CommandSyntaxException> {
    if !advancement.criteria.iter().any(|c| c == criterion) {
        return Err(
            ERROR_CRITERION_NOT_FOUND.create(advancement.name.clone(), criterion.to_owned())
        );
    }
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in players {
        tracker.track_bool(
            player,
            action.perform_criterion(source, player, advancement, criterion),
        );
    }
    let key = action.key();
    let (name, criterion) = (&advancement.name, criterion.to_owned());
    if tracker.total_value() == 0 {
        return Err(tracker.dispatch(
            ElementType::Any,
            |player, _| {
                Dynamic3CommandExceptionType::new(move |c: String, a: Component, p: Component| {
                    message(translatable!(
                        format!("commands.advancement.{key}.criterion.to.one.failure"),
                        c,
                        a,
                        p
                    ))
                })
                .create(
                    criterion.clone(),
                    name.clone(),
                    display_name(source, player),
                )
            },
            |player_count, _| {
                Dynamic3CommandExceptionType::new(move |c: String, a: Component, n: i32| {
                    message(translatable!(
                        format!("commands.advancement.{key}.criterion.to.many.failure"),
                        c,
                        a,
                        n
                    ))
                })
                .create(criterion.clone(), name.clone(), player_count)
            },
        ));
    }
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |player, _| {
            translatable!(
                format!("commands.advancement.{key}.criterion.to.one.success"),
                criterion.clone(),
                name.clone(),
                display_name(source, player)
            )
        },
        |player_count, _| {
            translatable!(
                format!("commands.advancement.{key}.criterion.to.many.success"),
                criterion.clone(),
                name.clone(),
                player_count
            )
        },
    )
}

/// `getAdvancements(context, target, mode)`.
fn get_advancements(context: &Ctx, target: AdvancementRef, mode: Mode) -> Vec<AdvancementRef> {
    let node = context
        .get_source()
        .with_server(|server| host::advancement_node(server, &target.id));
    let Some(node) = node else {
        return vec![target];
    };
    let mut advancements = Vec::new();
    if mode.parents() {
        let mut parent = node.parent;
        while let Some(p) = parent {
            parent = context
                .get_source()
                .with_server(|server| host::advancement_node(server, &p.id))
                .and_then(|n| n.parent);
            advancements.push(p);
        }
    }
    advancements.push(target);
    if mode.children() {
        for child in node.children {
            add_children(context, child, &mut advancements);
        }
    }
    advancements
}

fn add_children(context: &Ctx, child: AdvancementRef, output: &mut Vec<AdvancementRef>) {
    let node = context
        .get_source()
        .with_server(|server| host::advancement_node(server, &child.id));
    output.push(child);
    if let Some(node) = node {
        for grandchild in node.children {
            add_children(context, grandchild, output);
        }
    }
}
