//! `ExperienceCommand` (`experience`, `xp`).
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use super::command_response_tracker::CommandResponseTracker;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::register_command;
use mc_brigadier::arguments::{IntegerArgumentType, integer, integer_min};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::math::mth;
use std::sync::LazyLock;

static ERROR_SET_POINTS_INVALID: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.experience.set.points.invalid"));

/// `ExperienceCommand.Type`.
#[derive(Clone, Copy)]
enum Type {
    Points,
    Levels,
}

impl Type {
    fn name(self) -> &'static str {
        match self {
            Type::Points => "points",
            Type::Levels => "levels",
        }
    }

    /// `add`: `Player::giveExperiencePoints` / `ServerPlayer::giveExperienceLevels`.
    fn add(self, level: &mut mc_game_core::server::level::ServerLevel, player: &mut ServerPlayer, amount: i32) {
        match self {
            Type::Points => player.give_experience_points(level, amount),
            Type::Levels => player.give_experience_levels(level, amount),
        }
    }

    /// `set`: points above the level's capacity are rejected.
    fn set(self, player: &mut ServerPlayer, amount: i32) -> bool {
        match self {
            Type::Points => {
                if amount >= player.get_xp_needed_for_next_level() {
                    false
                } else {
                    player.set_experience_points(amount);
                    true
                }
            }
            Type::Levels => {
                player.set_experience_levels(amount);
                true
            }
        }
    }

    /// `query`.
    fn query(self, player: &ServerPlayer) -> i32 {
        match self {
            Type::Points => mth::floor_f32(player.player.experience_progress * player.get_xp_needed_for_next_level() as f32),
            Type::Levels => player.player.experience_level,
        }
    }
}

register_command!("ExperienceCommand", |ctx| {
    fn players(c: &CommandContext<CommandSourceStack>) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        EntityArgument::get_players(c, "target")
    }
    fn amount(c: &CommandContext<CommandSourceStack>) -> i32 {
        IntegerArgumentType::get_integer(c, "amount")
    }
    let command = ctx.dispatcher.register(
        literal("experience")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("add").then(
                    argument("target", EntityArgument::players()).then(
                        argument("amount", integer())
                            .executes(|c| add_experience(c.get_source(), players(c)?, amount(c), Type::Points))
                            .then(literal("points").executes(|c| add_experience(c.get_source(), players(c)?, amount(c), Type::Points)))
                            .then(literal("levels").executes(|c| add_experience(c.get_source(), players(c)?, amount(c), Type::Levels))),
                    ),
                ),
            )
            .then(
                literal("set").then(
                    argument("target", EntityArgument::players()).then(
                        argument("amount", integer_min(0))
                            .executes(|c| set_experience(c.get_source(), players(c)?, amount(c), Type::Points))
                            .then(literal("points").executes(|c| set_experience(c.get_source(), players(c)?, amount(c), Type::Points)))
                            .then(literal("levels").executes(|c| set_experience(c.get_source(), players(c)?, amount(c), Type::Levels))),
                    ),
                ),
            )
            .then(
                literal("query").then(
                    argument("target", EntityArgument::player())
                        .then(literal("points").executes(|c| query_experience(c.get_source(), EntityArgument::get_player(c, "target")?, Type::Points)))
                        .then(literal("levels").executes(|c| query_experience(c.get_source(), EntityArgument::get_player(c, "target")?, Type::Levels))),
                ),
            ),
    );
    ctx.dispatcher.register(literal("xp").requires(has_permission(LEVEL_GAMEMASTERS)).redirect(command));
});

fn query_experience(source: &CommandSourceStack, target: EntityRef, kind: Type) -> Result<i32, CommandSyntaxException> {
    let result = source.with_server(|server| {
        server.level(&target.dimension).and_then(|l| l.get_entity(target.id)).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()).map_or(0, |p| kind.query(p))
    });
    source.send_success(|| mc_game_core::translatable!(format!("commands.experience.query.{}", kind.name()), host::display_name_of(source, &target), result), false);
    Ok(result)
}

fn add_experience(source: &CommandSourceStack, players: Vec<EntityRef>, amount: i32, kind: Type) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in &players {
        source.with_server(|server| {
            if let Some(level) = server.level_mut(&player.dimension) {
                level.with_player(player.id, |level, p| kind.add(level, p, amount));
            }
        });
        tracker.track_one(player);
    }
    tracker.send_feedback_default(
        source,
        true,
        None,
        |player, _| mc_game_core::translatable!(format!("commands.experience.add.{}.success.single", kind.name()), amount, host::display_name_of(source, player)),
        |player_count, _| mc_game_core::translatable!(format!("commands.experience.add.{}.success.multiple", kind.name()), amount, player_count),
    )
}

fn set_experience(source: &CommandSourceStack, players: Vec<EntityRef>, amount: i32, kind: Type) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in &players {
        let set = source.with_server(|server| {
            server.level_mut(&player.dimension).and_then(|level| level.with_player(player.id, |_, p| kind.set(p, amount))).unwrap_or(false)
        });
        tracker.track_bool(player, set);
    }
    tracker.send_feedback_default(
        source,
        true,
        Some(&|| ERROR_SET_POINTS_INVALID.create()),
        |player, _| mc_game_core::translatable!(format!("commands.experience.set.{}.success.single", kind.name()), amount, host::display_name_of(source, player)),
        |player_count, _| mc_game_core::translatable!(format!("commands.experience.set.{}.success.multiple", kind.name()), amount, player_count),
    )
}
