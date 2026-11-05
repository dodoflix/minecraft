//! `SaveOnCommand`.
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{LEVEL_OWNERS, has_permission, literal};
use mc_brigadier::ArgumentBuilder;
use mc_game_core::{register_command, translatable};
use mc_brigadier::SimpleCommandExceptionType;
use std::sync::LazyLock;

static ERROR_ALREADY_ON: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.save.alreadyOn"));

register_command!("SaveOnCommand", |ctx| {
    ctx.dispatcher.register(literal("save-on").requires(has_permission(LEVEL_OWNERS)).executes(|c| {
        let source = c.get_source();
        if !source.with_server(|server| server.set_auto_save(true)) {
            Err(ERROR_ALREADY_ON.create())
        } else {
            source.send_success(|| translatable!("commands.save.enabled"), true);
            Ok(1)
        }
    }));
});
