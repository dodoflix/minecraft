//! `RandomCommand`.
use crate::commands::arguments::IdentifierArgument;
use crate::commands::arguments::min_max_bounds::Ints;
use crate::commands::arguments::range_argument::IntRangeArgument;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{BoolArgumentType, IntegerArgumentType, integer};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, LiteralArgumentBuilder, SimpleCommandExceptionType, Suggestions, SuggestionsBuilder};
use mc_core::ident::Identifier;
use mc_core::math::mth::random_between_inclusive;
use mc_core::random::RandomSource;
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_RANGE_TOO_LARGE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.random.error.range_too_large"));
static ERROR_RANGE_TOO_SMALL: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.random.error.range_too_small"));

type Ctx = CommandContext<CommandSourceStack>;

register_command!("RandomCommand", |ctx| {
    let seed = |c: &Ctx| IntegerArgumentType::get_integer(c, "seed");
    let world_seed = |c: &Ctx| BoolArgumentType::get_bool(c, "includeWorldSeed");
    let sequence_id = |c: &Ctx| BoolArgumentType::get_bool(c, "includeSequenceId");
    let sequence = |c: &Ctx| IdentifierArgument::get_id(c, "sequence");
    ctx.dispatcher.register(
        literal("random")
            .then(draw_random_value_tree("value", false))
            .then(draw_random_value_tree("roll", true))
            .then(
                literal("reset")
                    .requires(has_permission(LEVEL_GAMEMASTERS))
                    .then(
                        literal("*").executes(|c| Ok(reset_all_sequences(c.get_source()))).then(
                            argument("seed", integer())
                                .executes(move |c| Ok(reset_all_sequences_and_set_new_defaults(c.get_source(), seed(c), true, true)))
                                .then(
                                    argument("includeWorldSeed", BoolArgumentType::bool())
                                        .executes(move |c| Ok(reset_all_sequences_and_set_new_defaults(c.get_source(), seed(c), world_seed(c), true)))
                                        .then(argument("includeSequenceId", BoolArgumentType::bool()).executes(move |c| {
                                            Ok(reset_all_sequences_and_set_new_defaults(c.get_source(), seed(c), world_seed(c), sequence_id(c)))
                                        })),
                                ),
                        ),
                    )
                    .then(
                        argument("sequence", IdentifierArgument::id())
                            .suggests(|c, b| Ok(suggest_random_sequence(c, b)))
                            .executes(move |c| Ok(reset_sequence(c.get_source(), &sequence(c), None)))
                            .then(
                                argument("seed", integer())
                                    .executes(move |c| Ok(reset_sequence(c.get_source(), &sequence(c), Some((seed(c), true, true)))))
                                    .then(
                                        argument("includeWorldSeed", BoolArgumentType::bool())
                                            .executes(move |c| Ok(reset_sequence(c.get_source(), &sequence(c), Some((seed(c), world_seed(c), true)))))
                                            .then(argument("includeSequenceId", BoolArgumentType::bool()).executes(move |c| {
                                                Ok(reset_sequence(c.get_source(), &sequence(c), Some((seed(c), world_seed(c), sequence_id(c)))))
                                            })),
                                    ),
                            ),
                    ),
            ),
    );
});

fn draw_random_value_tree(name: &'static str, announce: bool) -> LiteralArgumentBuilder<CommandSourceStack> {
    literal(name).then(
        argument("range", IntRangeArgument::int_range())
            .executes(move |c| random_sample(c.get_source(), IntRangeArgument::get_range(c, "range"), None, announce))
            .then(
                argument("sequence", IdentifierArgument::id())
                    .suggests(|c, b| Ok(suggest_random_sequence(c, b)))
                    .requires(has_permission(LEVEL_GAMEMASTERS))
                    .executes(move |c| {
                        random_sample(c.get_source(), IntRangeArgument::get_range(c, "range"), Some(IdentifierArgument::get_id(c, "sequence")), announce)
                    }),
            ),
    )
}

fn suggest_random_sequence(context: &Ctx, builder: SuggestionsBuilder) -> Suggestions {
    let mut result = Vec::new();
    context.get_source().with_server(|server| {
        if let Some(sequences) = server.saved_data.peek(&server.saved_types.random_sequences) {
            sequences.for_all_sequences(|key, _| result.push(key.to_string()));
        }
    });
    suggest(result, builder)
}

fn random_sample(source: &CommandSourceStack, range: Ints, sequence: Option<Identifier>, announce: bool) -> Result<i32, CommandSyntaxException> {
    let min = range.bounds.min.unwrap_or(i32::MIN);
    let max = range.bounds.max.unwrap_or(i32::MAX);
    let span = i64::from(max) - i64::from(min);
    // The random source is fetched (creating the sequence) before the range checks, like vanilla.
    let value = source.with_server(|server| {
        let seed = server.world_gen_settings.options.seed;
        let mut sequences_random;
        let level_random;
        let random: &mut dyn RandomSource = match &sequence {
            Some(id) => {
                sequences_random = server.saved_data.compute_if_absent(&server.saved_types.random_sequences).get(id, seed);
                &mut sequences_random
            }
            None => {
                level_random = server.level_mut(source.get_level())?;
                &mut level_random.tick_state.random
            }
        };
        if span == 0 {
            return Some(Err(ERROR_RANGE_TOO_SMALL.create()));
        }
        if span >= 2147483647 {
            return Some(Err(ERROR_RANGE_TOO_LARGE.create()));
        }
        Some(Ok(random_between_inclusive(random, min, max)))
    });
    let value = value.expect("source level exists")?;
    if announce {
        let message = translatable!("commands.random.roll", source.get_display_name(), value, min, max);
        source.with_server(|server| server.player_list.broadcast_system_message(&message, false));
    } else {
        source.send_success(|| translatable!("commands.random.sample.success", value), false);
    }
    Ok(value)
}

fn reset_sequence(source: &CommandSourceStack, sequence: &Identifier, args: Option<(i32, bool, bool)>) -> i32 {
    source.with_server(|server| {
        let seed = server.world_gen_settings.options.seed;
        let sequences = server.saved_data.compute_if_absent(&server.saved_types.random_sequences);
        match args {
            None => sequences.reset(sequence, seed),
            Some((salt, include_world_seed, include_sequence_id)) => sequences.reset_with(sequence, seed, salt, include_world_seed, include_sequence_id),
        }
    });
    source.send_success(|| translatable!("commands.random.reset.success", Component::literal(sequence.to_string())), false);
    1
}

fn reset_all_sequences(source: &CommandSourceStack) -> i32 {
    let count = source.with_server(|server| server.saved_data.compute_if_absent(&server.saved_types.random_sequences).clear());
    source.send_success(|| translatable!("commands.random.reset.all.success", count), false);
    count
}

fn reset_all_sequences_and_set_new_defaults(source: &CommandSourceStack, salt: i32, include_world_seed: bool, include_sequence_id: bool) -> i32 {
    let count = source.with_server(|server| {
        let sequences = server.saved_data.compute_if_absent(&server.saved_types.random_sequences);
        sequences.set_seed_defaults(salt, include_world_seed, include_sequence_id);
        sequences.clear()
    });
    source.send_success(|| translatable!("commands.random.reset.all.success", count), false);
    count
}
