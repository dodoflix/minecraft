//! `EffectCommands` (`/effect give|clear`).
use super::command_response_tracker::CommandResponseTracker;
use mc_brigadier::arguments::{BoolArgumentType, IntegerArgumentType, integer_range};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType,
};
use mc_data::MobEffect;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::resource_arguments::ResourceArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_GIVE_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.effect.give.failed"));
static ERROR_CLEAR_EVERYTHING_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.effect.clear.everything.failed"));
static ERROR_CLEAR_SPECIFIC_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.effect.clear.specific.failed"));

register_command!("EffectCommands", |ctx| {
    fn targets(
        c: &CommandContext<CommandSourceStack>,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        EntityArgument::get_entities(c, "targets")
    }
    fn effect(c: &CommandContext<CommandSourceStack>) -> Result<MobEffect, CommandSyntaxException> {
        ResourceArgument::get_mob_effect(c, "effect")
    }
    fn seconds(c: &CommandContext<CommandSourceStack>) -> Option<i32> {
        Some(IntegerArgumentType::get_integer(c, "seconds"))
    }
    fn amplifier(c: &CommandContext<CommandSourceStack>) -> i32 {
        IntegerArgumentType::get_integer(c, "amplifier")
    }
    fn hide_particles(c: &CommandContext<CommandSourceStack>) -> bool {
        !BoolArgumentType::get_bool(c, "hideParticles")
    }
    ctx.dispatcher.register(
        literal("effect")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("clear")
                    .executes(|c| {
                        clear_effects(
                            c.get_source(),
                            vec![host::source_entity_ref(c.get_source())?],
                        )
                    })
                    .then(
                        argument("targets", EntityArgument::entities())
                            .executes(|c| clear_effects(c.get_source(), targets(c)?))
                            .then(
                                argument(
                                    "effect",
                                    ResourceArgument::resource(ctx.build_context, "mob_effect"),
                                )
                                .executes(|c| {
                                    clear_effect(c.get_source(), targets(c)?, effect(c)?)
                                }),
                            ),
                    ),
            )
            .then(
                literal("give").then(
                    argument("targets", EntityArgument::entities()).then(
                        argument(
                            "effect",
                            ResourceArgument::resource(ctx.build_context, "mob_effect"),
                        )
                        .executes(|c| {
                            give_effect(c.get_source(), targets(c)?, effect(c)?, None, 0, true)
                        })
                        .then(
                            argument("seconds", integer_range(1, 1_000_000))
                                .executes(|c| {
                                    give_effect(
                                        c.get_source(),
                                        targets(c)?,
                                        effect(c)?,
                                        seconds(c),
                                        0,
                                        true,
                                    )
                                })
                                .then(
                                    argument("amplifier", integer_range(0, 255))
                                        .executes(|c| {
                                            give_effect(
                                                c.get_source(),
                                                targets(c)?,
                                                effect(c)?,
                                                seconds(c),
                                                amplifier(c),
                                                true,
                                            )
                                        })
                                        .then(
                                            argument("hideParticles", BoolArgumentType::bool())
                                                .executes(|c| {
                                                    give_effect(
                                                        c.get_source(),
                                                        targets(c)?,
                                                        effect(c)?,
                                                        seconds(c),
                                                        amplifier(c),
                                                        hide_particles(c),
                                                    )
                                                }),
                                        ),
                                ),
                        )
                        .then(
                            literal("infinite")
                                .executes(|c| {
                                    give_effect(
                                        c.get_source(),
                                        targets(c)?,
                                        effect(c)?,
                                        Some(-1),
                                        0,
                                        true,
                                    )
                                })
                                .then(
                                    argument("amplifier", integer_range(0, 255))
                                        .executes(|c| {
                                            give_effect(
                                                c.get_source(),
                                                targets(c)?,
                                                effect(c)?,
                                                Some(-1),
                                                amplifier(c),
                                                true,
                                            )
                                        })
                                        .then(
                                            argument("hideParticles", BoolArgumentType::bool())
                                                .executes(|c| {
                                                    give_effect(
                                                        c.get_source(),
                                                        targets(c)?,
                                                        effect(c)?,
                                                        Some(-1),
                                                        amplifier(c),
                                                        hide_particles(c),
                                                    )
                                                }),
                                        ),
                                ),
                        ),
                    ),
                ),
            ),
    );
});

fn effect_name(effect: MobEffect) -> Component {
    Component::translatable(effect.data().description_id, Vec::new())
}

/// `computeDurationInTicks(seconds, effect)`.
fn compute_duration_in_ticks(seconds: Option<i32>, effect: MobEffect) -> i32 {
    match seconds {
        Some(seconds) => {
            if effect.data().instantaneous {
                seconds
            } else if seconds == -1 {
                -1
            } else {
                seconds.wrapping_mul(20)
            }
        }
        None => {
            if effect.data().instantaneous {
                1
            } else {
                600
            }
        }
    }
}

fn is_living(source: &CommandSourceStack, entity: &EntityRef) -> bool {
    source.with_server(|server| {
        host::get_entity(server, entity).is_some_and(|e| e.living().is_some())
    })
}

fn give_effect(
    source: &CommandSourceStack,
    entities: Vec<EntityRef>,
    effect: MobEffect,
    seconds: Option<i32>,
    amplifier: i32,
    particles: bool,
) -> Result<i32, CommandSyntaxException> {
    let duration = compute_duration_in_ticks(seconds, effect);
    let source_entity = source.get_entity().map(|e| e.id);
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for entity in &entities {
        if !is_living(source, entity) {
            continue;
        }
        let added = source.with_server(|server| {
            let instance =
                MobEffectInstance::with_flags(effect, duration, amplifier, false, particles);
            server.level_mut(&entity.dimension).and_then(|level| {
                level.with_entity(entity.id, |level, e| {
                    e.add_effect(level, instance, source_entity)
                })
            })
        });
        tracker.track_bool(entity, added == Some(true));
    }
    tracker.send_feedback_default(
        source,
        true,
        Some(&|| ERROR_GIVE_FAILED.create()),
        |entity, _| {
            translatable!(
                "commands.effect.give.success.single",
                effect_name(effect),
                host::display_name_of(source, entity),
                duration / 20
            )
        },
        |entity_count, _| {
            translatable!(
                "commands.effect.give.success.multiple",
                effect_name(effect),
                entity_count,
                duration / 20
            )
        },
    )
}

fn clear_effects(
    source: &CommandSourceStack,
    entities: Vec<EntityRef>,
) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for entity in &entities {
        if !is_living(source, entity) {
            continue;
        }
        let removed = source.with_server(|server| {
            server.level_mut(&entity.dimension).and_then(|level| {
                level.with_entity(entity.id, |level, e| e.remove_all_effects(level))
            })
        });
        tracker.track_bool(entity, removed == Some(true));
    }
    tracker.send_feedback_default(
        source,
        true,
        Some(&|| ERROR_CLEAR_EVERYTHING_FAILED.create()),
        |entity, _| {
            translatable!(
                "commands.effect.clear.everything.success.single",
                host::display_name_of(source, entity)
            )
        },
        |entity_count, _| {
            translatable!(
                "commands.effect.clear.everything.success.multiple",
                entity_count
            )
        },
    )
}

fn clear_effect(
    source: &CommandSourceStack,
    entities: Vec<EntityRef>,
    effect: MobEffect,
) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for entity in &entities {
        if !is_living(source, entity) {
            continue;
        }
        let removed = source.with_server(|server| {
            server.level_mut(&entity.dimension).and_then(|level| {
                level.with_entity(entity.id, |level, e| e.remove_effect(level, effect))
            })
        });
        tracker.track_bool(entity, removed == Some(true));
    }
    tracker.send_feedback_default(
        source,
        true,
        Some(&|| ERROR_CLEAR_SPECIFIC_FAILED.create()),
        |entity, _| {
            translatable!(
                "commands.effect.clear.specific.success.single",
                effect_name(effect),
                host::display_name_of(source, entity)
            )
        },
        |entity_count, _| {
            translatable!(
                "commands.effect.clear.specific.success.multiple",
                effect_name(effect),
                entity_count
            )
        },
    )
}
