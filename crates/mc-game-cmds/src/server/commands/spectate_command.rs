//! `SpectateCommand`.
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType, SimpleCommandExceptionType};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_SELF: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.spectate.self"));
static ERROR_NOT_SPECTATOR: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|s: Component| message(translatable!("commands.spectate.not_spectator", s))));
static ERROR_CANNOT_SPECTATE: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|s: Component| message(translatable!("commands.spectate.cannot_spectate", s))));

register_command!("SpectateCommand", |ctx| {
    fn source_player(source: &CommandSourceStack) -> Result<EntityRef, CommandSyntaxException> {
        source.get_player_or_exception()?;
        host::source_entity_ref(source)
    }
    ctx.dispatcher.register(
        literal("spectate")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| spectate(c.get_source(), None, source_player(c.get_source())?))
            .then(
                argument("target", EntityArgument::entity())
                    .executes(|c| spectate(c.get_source(), Some(EntityArgument::get_entity(c, "target")?), source_player(c.get_source())?))
                    .then(argument("player", EntityArgument::player()).executes(|c| {
                        spectate(c.get_source(), Some(EntityArgument::get_entity(c, "target")?), EntityArgument::get_player(c, "player")?)
                    })),
            ),
    );
});

fn spectate(source: &CommandSourceStack, target: Option<EntityRef>, player: EntityRef) -> Result<i32, CommandSyntaxException> {
    if target.as_ref() == Some(&player) {
        return Err(ERROR_SELF.create());
    }
    if !source.with_server(|server| host::get_entity(server, &player).is_some_and(|p| p.is_spectator())) {
        return Err(ERROR_NOT_SPECTATOR.create(host::display_name_of(source, &player)));
    }
    if let Some(target) = &target
        && source.with_server(|server| host::get_entity(server, target).is_some_and(|t| t.get_type().data().client_tracking_range == 0))
    {
        return Err(ERROR_CANNOT_SPECTATE.create(host::display_name_of(source, target)));
    }
    source.with_server(|server| {
        if let Some(level) = server.level_mut(&player.dimension) {
            level.with_player(player.id, |level, p| p.set_camera(level, target.as_ref().map(|t| t.id)));
        }
    });
    match &target {
        Some(target) => source.send_success(|| translatable!("commands.spectate.success.started", host::display_name_of(source, target)), false),
        None => source.send_success(|| translatable!("commands.spectate.success.stopped"), false),
    }
    Ok(1)
}
