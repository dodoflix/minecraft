//! `TeamCommand`.
use super::command_response_tracker::CommandResponseTracker;
use super::scoreboard_command::with_scoreboard;
use crate::commands::arguments::scores::{ScoreHolder, suggest_score_holders};
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::{ComponentArgument, ScoreHolderArgument, TeamArgument, TeamColor, TeamColorArgument};
use crate::commands::message::simple_translatable;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::world::scores::{CollisionRule, Visibility};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{BoolArgumentType, StringArgumentType};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, SimpleCommandExceptionType};
use mc_text::util::{default_separator, format_list, format_string_list};
use mc_text::Component;
use std::sync::LazyLock;

macro_rules! errors {
    ($($name:ident = $key:literal;)*) => {
        $(static $name: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable($key));)*
    };
}

errors! {
    ERROR_TEAM_ALREADY_EXISTS = "commands.team.add.duplicate";
    ERROR_TEAM_ALREADY_EMPTY = "commands.team.empty.unchanged";
    ERROR_TEAM_ALREADY_NAME = "commands.team.option.name.unchanged";
    ERROR_TEAM_ALREADY_COLOR = "commands.team.option.color.unchanged";
    ERROR_TEAM_ALREADY_FRIENDLYFIRE_ENABLED = "commands.team.option.friendlyfire.alreadyEnabled";
    ERROR_TEAM_ALREADY_FRIENDLYFIRE_DISABLED = "commands.team.option.friendlyfire.alreadyDisabled";
    ERROR_TEAM_ALREADY_FRIENDLYINVISIBLES_ENABLED = "commands.team.option.seeFriendlyInvisibles.alreadyEnabled";
    ERROR_TEAM_ALREADY_FRIENDLYINVISIBLES_DISABLED = "commands.team.option.seeFriendlyInvisibles.alreadyDisabled";
    ERROR_TEAM_NAMETAG_VISIBLITY_UNCHANGED = "commands.team.option.nametagVisibility.unchanged";
    ERROR_TEAM_DEATH_MESSAGE_VISIBLITY_UNCHANGED = "commands.team.option.deathMessageVisibility.unchanged";
    ERROR_TEAM_COLLISION_UNCHANGED = "commands.team.option.collisionRule.unchanged";
}

register_command!("TeamCommand", |ctx| {
    fn team(c: &mc_brigadier::CommandContext<CommandSourceStack>) -> Result<String, CommandSyntaxException> {
        TeamArgument::get_team(c, "team")
    }
    fn members(c: &mc_brigadier::CommandContext<CommandSourceStack>) -> Result<Vec<ScoreHolder>, CommandSyntaxException> {
        ScoreHolderArgument::get_names_with_default_wildcard(c, "members")
    }

    let mut nametag_visibility = literal("nametagVisibility");
    let mut death_message_visibility = literal("deathMessageVisibility");
    for visibility in [Visibility::Never, Visibility::HideForOtherTeams, Visibility::HideForOwnTeam, Visibility::Always] {
        nametag_visibility = nametag_visibility
            .then(literal(visibility.get_serialized_name()).executes(move |c| set_name_tag_visibility(c.get_source(), &team(c)?, visibility)));
        death_message_visibility = death_message_visibility
            .then(literal(visibility.get_serialized_name()).executes(move |c| set_death_message_visibility(c.get_source(), &team(c)?, visibility)));
    }
    let mut collision_rule = literal("collisionRule");
    for rule in [CollisionRule::Never, CollisionRule::PushOwnTeam, CollisionRule::PushOtherTeams, CollisionRule::Always] {
        collision_rule = collision_rule.then(literal(rule.get_serialized_name()).executes(move |c| set_collision(c.get_source(), &team(c)?, rule)));
    }

    ctx.dispatcher.register(
        literal("team")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("list")
                    .executes(|c| Ok(list_teams(c.get_source())))
                    .then(argument("team", TeamArgument::team()).executes(|c| Ok(list_members(c.get_source(), &team(c)?)))),
            )
            .then(
                literal("add").then(
                    argument("team", StringArgumentType::word())
                        .executes(|c| {
                            let name = StringArgumentType::get_string(c, "team");
                            create_team(c.get_source(), name, Component::literal(name))
                        })
                        .then(argument("displayName", ComponentArgument::text_component()).executes(|c| {
                            create_team(
                                c.get_source(),
                                StringArgumentType::get_string(c, "team"),
                                ComponentArgument::get_resolved_component_for_source(c, "displayName")?,
                            )
                        })),
                ),
            )
            .then(literal("remove").then(argument("team", TeamArgument::team()).executes(|c| Ok(delete_team(c.get_source(), &team(c)?)))))
            .then(literal("empty").then(argument("team", TeamArgument::team()).executes(|c| empty_team(c.get_source(), &team(c)?))))
            .then(
                literal("join").then(
                    argument("team", TeamArgument::team())
                        .executes(|c| {
                            let team = team(c)?;
                            let entity = c.get_source().get_entity_or_exception()?;
                            let entity = EntityRef { dimension: c.get_source().get_level().clone(), id: entity.id };
                            let holder = c.get_source().with_server(|server| ScoreHolder::of_entity(server, &entity)).expect("source entity is alive");
                            join_team(c.get_source(), &team, vec![holder])
                        })
                        .then(
                            argument("members", ScoreHolderArgument::score_holders())
                                .suggests_provider(Some(suggest_score_holders()))
                                .executes(|c| join_team(c.get_source(), &team(c)?, members(c)?)),
                        ),
                ),
            )
            .then(
                literal("leave").then(
                    argument("members", ScoreHolderArgument::score_holders())
                        .suggests_provider(Some(suggest_score_holders()))
                        .executes(|c| leave_team(c.get_source(), members(c)?)),
                ),
            )
            .then(
                literal("modify").then(
                    argument("team", TeamArgument::team())
                        .then(literal("displayName").then(argument("displayName", ComponentArgument::text_component()).executes(|c| {
                            set_display_name(c.get_source(), &team(c)?, ComponentArgument::get_resolved_component_for_source(c, "displayName")?)
                        })))
                        .then(
                            literal("color")
                                .then(literal("reset").executes(|c| clear_color(c.get_source(), &team(c)?)))
                                .then(argument("value", TeamColorArgument::team_color()).executes(|c| {
                                    set_color(c.get_source(), &team(c)?, TeamColorArgument::get_team_color(c, "value"))
                                })),
                        )
                        .then(literal("friendlyFire").then(
                            argument("allowed", BoolArgumentType::bool()).executes(|c| set_friendly_fire(c.get_source(), &team(c)?, BoolArgumentType::get_bool(c, "allowed"))),
                        ))
                        .then(literal("seeFriendlyInvisibles").then(
                            argument("allowed", BoolArgumentType::bool()).executes(|c| set_friendly_sight(c.get_source(), &team(c)?, BoolArgumentType::get_bool(c, "allowed"))),
                        ))
                        .then(nametag_visibility)
                        .then(death_message_visibility)
                        .then(collision_rule)
                        .then(literal("prefix").then(argument("prefix", ComponentArgument::text_component()).executes(|c| {
                            set_prefix(c.get_source(), &team(c)?, ComponentArgument::get_resolved_component_for_source(c, "prefix")?)
                        })))
                        .then(literal("suffix").then(argument("suffix", ComponentArgument::text_component()).executes(|c| {
                            set_suffix(c.get_source(), &team(c)?, ComponentArgument::get_resolved_component_for_source(c, "suffix")?)
                        }))),
                ),
            ),
    );
});

/// `team.getFormattedDisplayName()`.
fn formatted(source: &CommandSourceStack, team: &str) -> Component {
    source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").get_formatted_display_name())
}

fn leave_team(source: &CommandSourceStack, members: Vec<ScoreHolder>) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for member in members {
            let left = scoreboard.remove_player_from_team(&member.scoreboard_name);
            tracker.track_bool(member, left);
        }
    });
    tracker.send_feedback(
        source,
        true,
        None,
        |holder, _| translatable!("commands.team.leave.success.single", holder.get_feedback_display_name()),
        |count, _| translatable!("commands.team.leave.success.multiple", count),
    )
}

fn join_team(source: &CommandSourceStack, team: &str, members: Vec<ScoreHolder>) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::new();
    with_scoreboard(source, |scoreboard| {
        for member in members {
            let joined = scoreboard.add_player_to_team(&member.scoreboard_name, team);
            tracker.track_bool(member, joined);
        }
    });
    let team = formatted(source, team);
    tracker.send_feedback(
        source,
        true,
        None,
        |holder, _| translatable!("commands.team.join.success.single", holder.get_feedback_display_name(), team.clone()),
        |count, _| translatable!("commands.team.join.success.multiple", count, team.clone()),
    )
}

fn set_name_tag_visibility(source: &CommandSourceStack, team: &str, visibility: Visibility) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").get_name_tag_visibility()) == visibility {
        return Err(ERROR_TEAM_NAMETAG_VISIBLITY_UNCHANGED.create());
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_name_tag_visibility(visibility));
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.option.nametagVisibility.success", name, visibility.get_display_name()), true);
    Ok(0)
}

fn set_death_message_visibility(source: &CommandSourceStack, team: &str, visibility: Visibility) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").get_death_message_visibility()) == visibility {
        return Err(ERROR_TEAM_DEATH_MESSAGE_VISIBLITY_UNCHANGED.create());
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_death_message_visibility(visibility));
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.option.deathMessageVisibility.success", name, visibility.get_display_name()), true);
    Ok(0)
}

fn set_collision(source: &CommandSourceStack, team: &str, collision: CollisionRule) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").get_collision_rule()) == collision {
        return Err(ERROR_TEAM_COLLISION_UNCHANGED.create());
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_collision_rule(collision));
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.option.collisionRule.success", name, collision.get_display_name()), true);
    Ok(0)
}

fn set_friendly_sight(source: &CommandSourceStack, team: &str, allowed: bool) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").can_see_friendly_invisibles()) == allowed {
        return Err(if allowed { ERROR_TEAM_ALREADY_FRIENDLYINVISIBLES_ENABLED.create() } else { ERROR_TEAM_ALREADY_FRIENDLYINVISIBLES_DISABLED.create() });
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_see_friendly_invisibles(allowed));
    let name = formatted(source, team);
    let key = if allowed { "commands.team.option.seeFriendlyInvisibles.enabled" } else { "commands.team.option.seeFriendlyInvisibles.disabled" };
    source.send_success(|| translatable!(key, name), true);
    Ok(0)
}

fn set_friendly_fire(source: &CommandSourceStack, team: &str, allowed: bool) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").is_allow_friendly_fire()) == allowed {
        return Err(if allowed { ERROR_TEAM_ALREADY_FRIENDLYFIRE_ENABLED.create() } else { ERROR_TEAM_ALREADY_FRIENDLYFIRE_DISABLED.create() });
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_allow_friendly_fire(allowed));
    let name = formatted(source, team);
    let key = if allowed { "commands.team.option.friendlyfire.enabled" } else { "commands.team.option.friendlyfire.disabled" };
    source.send_success(|| translatable!(key, name), true);
    Ok(0)
}

fn set_display_name(source: &CommandSourceStack, team: &str, display_name: Component) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| *s.scoreboard.read().get_player_team(team).expect("team exists").get_display_name() == display_name) {
        return Err(ERROR_TEAM_ALREADY_NAME.create());
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_display_name(display_name));
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.option.name.success", name), true);
    Ok(0)
}

fn set_color(source: &CommandSourceStack, team: &str, color: TeamColor) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").get_color()) == Some(color) {
        return Err(ERROR_TEAM_ALREADY_COLOR.create());
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_color(Some(color)));
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.option.color.success", name, color.get_serialized_name()), true);
    Ok(0)
}

fn clear_color(source: &CommandSourceStack, team: &str) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(team).expect("team exists").get_color()).is_none() {
        return Err(ERROR_TEAM_ALREADY_COLOR.create());
    }
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_color(None));
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.option.color.clear.success", name), true);
    Ok(0)
}

fn empty_team(source: &CommandSourceStack, team: &str) -> Result<i32, CommandSyntaxException> {
    // `Lists.newArrayList(team.getPlayers())`: the HashSet's iteration order.
    let members: Vec<String> = source.with_server(|s| {
        s.scoreboard.read().get_player_team(team).expect("team exists").get_players().iter_java_order().into_iter().map(str::to_owned).collect()
    });
    if members.is_empty() {
        return Err(ERROR_TEAM_ALREADY_EMPTY.create());
    }
    with_scoreboard(source, |scoreboard| {
        for member in &members {
            scoreboard.remove_player_from_team_of(member, team).unwrap_or_else(|e| panic!("{e}"));
        }
    });
    let name = formatted(source, team);
    source.send_success(|| translatable!("commands.team.empty.success", members.len() as i32, name), true);
    Ok(members.len() as i32)
}

fn delete_team(source: &CommandSourceStack, team: &str) -> i32 {
    // The removed team object still renders its name in the feedback.
    let name = formatted(source, team);
    let remaining = with_scoreboard(source, |scoreboard| {
        scoreboard.remove_player_team(team);
        scoreboard.get_player_teams().count() as i32
    });
    source.send_success(|| translatable!("commands.team.remove.success", name), true);
    remaining
}

fn create_team(source: &CommandSourceStack, name: &str, display_name: Component) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|s| s.scoreboard.read().get_player_team(name).is_some()) {
        return Err(ERROR_TEAM_ALREADY_EXISTS.create());
    }
    let count = with_scoreboard(source, |scoreboard| {
        scoreboard.add_player_team(name);
        scoreboard.team_mut(name).expect("just added").set_display_name(display_name);
        scoreboard.get_player_teams().count() as i32
    });
    let formatted = formatted(source, name);
    source.send_success(|| translatable!("commands.team.add.success", formatted), true);
    Ok(count)
}

fn list_members(source: &CommandSourceStack, team: &str) -> i32 {
    let members: Vec<String> = source.with_server(|s| {
        s.scoreboard.read().get_player_team(team).expect("team exists").get_players().iter_java_order().into_iter().map(str::to_owned).collect()
    });
    let name = formatted(source, team);
    if members.is_empty() {
        source.send_success(|| translatable!("commands.team.list.members.empty", name), false);
    } else {
        source.send_success(|| translatable!("commands.team.list.members.success", name, members.len() as i32, format_string_list(&members)), false);
    }
    members.len() as i32
}

fn list_teams(source: &CommandSourceStack) -> i32 {
    let teams: Vec<Component> = source.with_server(|s| s.scoreboard.read().get_player_teams().map(|t| t.get_formatted_display_name()).collect());
    if teams.is_empty() {
        source.send_success(|| translatable!("commands.team.list.teams.empty"), false);
    } else {
        source.send_success(
            || translatable!("commands.team.list.teams.success", teams.len() as i32, format_list(teams.iter(), &default_separator(), |t| t.clone())),
            false,
        );
    }
    teams.len() as i32
}

fn set_prefix(source: &CommandSourceStack, team: &str, prefix: Component) -> Result<i32, CommandSyntaxException> {
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_player_prefix(Some(prefix.clone())));
    source.send_success(|| translatable!("commands.team.option.prefix.success", prefix), false);
    Ok(1)
}

fn set_suffix(source: &CommandSourceStack, team: &str, suffix: Component) -> Result<i32, CommandSyntaxException> {
    with_scoreboard(source, |sb| sb.team_mut(team).expect("team exists").set_player_suffix(Some(suffix.clone())));
    source.send_success(|| translatable!("commands.team.option.suffix.success", suffix), false);
    Ok(1)
}
