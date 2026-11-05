//! `PostEffectCommand`.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, IdentifierArgument};
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::synchronization::suggestion_providers;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_ADD_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.posteffect.add.failed"));
static ERROR_CLEAR_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.posteffect.clear.failed"));
static ERROR_REMOVE_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.posteffect.remove.failed"));

register_command!("PostEffectCommand", |ctx| {
    let post_effect = || argument("posteffect", IdentifierArgument::id()).suggests_provider(Some(suggestion_providers::post_effects()));
    ctx.dispatcher.register(
        literal("posteffect")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("add").then(argument("targets", EntityArgument::players()).then(post_effect().executes(|c| {
                add_post_effect(c.get_source(), EntityArgument::get_players(c, "targets")?, IdentifierArgument::get_id(c, "posteffect"))
            }))))
            .then(literal("clear").then(
                argument("targets", EntityArgument::players()).executes(|c| clear_post_effect(c.get_source(), EntityArgument::get_players(c, "targets")?)),
            ))
            .then(literal("list").then(argument("target", EntityArgument::player()).executes(|c| list_post_effects(c.get_source(), EntityArgument::get_player(c, "target")?))))
            .then(literal("remove").then(argument("targets", EntityArgument::players()).then(post_effect().executes(|c| {
                remove_post_effect(c.get_source(), EntityArgument::get_players(c, "targets")?, IdentifierArgument::get_id(c, "posteffect"))
            })))),
    );
});

/// Runs `f` on the live `ServerPlayer` of `player`.
fn with_player<R>(source: &CommandSourceStack, player: &EntityRef, f: impl FnOnce(&mut ServerPlayer) -> R) -> Option<R> {
    source.with_server(|server| {
        let entity = server.level_mut(&player.dimension)?.get_entity_mut(player.id)?;
        entity.as_any_mut().downcast_mut::<ServerPlayer>().map(f)
    })
}

/// `player.getDisplayName()`.
fn display_name(source: &CommandSourceStack, player: &EntityRef) -> Component {
    source.with_server(|server| host::get_entity(server, player).map_or_else(Component::empty, |e| host::entity_display_name(server, e)))
}

fn add_post_effect(source: &CommandSourceStack, players: Vec<EntityRef>, post_effect: Identifier) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in players {
        let changed = with_player(source, &player, |p| p.add_post_effect(&post_effect)).unwrap_or(false);
        tracker.track_bool(&player, changed);
    }
    let arg = Component::literal(post_effect.to_string());
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        Some(|| ERROR_ADD_FAILED.create()),
        |player, _| translatable!("commands.posteffect.add.success.single", arg.clone(), display_name(source, player)),
        |count, _| translatable!("commands.posteffect.add.success.multiple", arg.clone(), count),
    )
}

fn clear_post_effect(source: &CommandSourceStack, players: Vec<EntityRef>) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in players {
        let changed = with_player(source, &player, |p| p.clear_post_effects()).unwrap_or(false);
        tracker.track_bool(&player, changed);
    }
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        Some(|| ERROR_CLEAR_FAILED.create()),
        |player, _| translatable!("commands.posteffect.clear.success.single", display_name(source, player)),
        |count, _| translatable!("commands.posteffect.clear.success.multiple", count),
    )
}

fn list_post_effects(source: &CommandSourceStack, player: EntityRef) -> Result<i32, CommandSyntaxException> {
    let post_effects = with_player(source, &player, |p| p.get_post_effects().to_vec()).unwrap_or_default();
    if post_effects.is_empty() {
        source.send_success(|| translatable!("commands.posteffect.list.empty", display_name(source, &player)), false);
    } else {
        let names = post_effects.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        source.send_success(|| translatable!("commands.posteffect.list.success", display_name(source, &player), post_effects.len() as i32, names), false);
    }
    Ok(post_effects.len() as i32)
}

fn remove_post_effect(source: &CommandSourceStack, players: Vec<EntityRef>, post_effect: Identifier) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for player in players {
        let changed = with_player(source, &player, |p| p.remove_post_effect(&post_effect)).unwrap_or(false);
        tracker.track_bool(&player, changed);
    }
    let arg = Component::literal(post_effect.to_string());
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        Some(|| ERROR_REMOVE_FAILED.create()),
        |player, _| translatable!("commands.posteffect.remove.success.single", arg.clone(), display_name(source, player)),
        |count, _| translatable!("commands.posteffect.remove.success.multiple", arg.clone(), count),
    )
}
