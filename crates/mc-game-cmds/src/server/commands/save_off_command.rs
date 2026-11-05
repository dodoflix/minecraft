//! `SaveOffCommand`.
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{LEVEL_OWNERS, has_permission, literal};
use mc_brigadier::ArgumentBuilder;
use mc_game_core::{register_command, translatable};
use mc_brigadier::SimpleCommandExceptionType;
use std::sync::LazyLock;

static ERROR_ALREADY_OFF: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.save.alreadyOff"));

register_command!("SaveOffCommand", |ctx| {
    ctx.dispatcher.register(literal("save-off").requires(has_permission(LEVEL_OWNERS)).executes(|c| {
        let source = c.get_source();
        if !source.with_server(|server| server.set_auto_save(false)) {
            Err(ERROR_ALREADY_OFF.create())
        } else {
            source.send_success(|| translatable!("commands.save.disabled"), true);
            Ok(1)
        }
    }));
});
