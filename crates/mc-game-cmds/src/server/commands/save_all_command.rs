//! `SaveAllCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_OWNERS, has_permission, literal};
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.save.failed"));

register_command!("SaveAllCommand", |ctx| {
    ctx.dispatcher.register(
        literal("save-all")
            .requires(has_permission(LEVEL_OWNERS))
            .executes(|c| save_all(c.get_source(), false))
            .then(literal("flush").executes(|c| save_all(c.get_source(), true))),
    );
});

fn save_all(source: &CommandSourceStack, flush: bool) -> Result<i32, CommandSyntaxException> {
    source.send_success(|| translatable!("commands.save.saving"), false);
    let success = source.with_server(|server| server.save_everything(true, flush, true));
    if !success {
        Err(ERROR_FAILED.create())
    } else {
        source.send_success(|| translatable!("commands.save.success"), true);
        Ok(1)
    }
}
