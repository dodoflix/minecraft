//! `SetPlayerIdleTimeoutCommand`.
use crate::commands::{LEVEL_ADMINS, argument, has_permission, literal};
use crate::{register_command, translatable};
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::arguments::{IntegerArgumentType, integer_min};

register_command!("SetPlayerIdleTimeoutCommand", |ctx| {
    ctx.dispatcher.register(literal("setidletimeout").requires(has_permission(LEVEL_ADMINS)).then(argument("minutes", integer_min(0)).executes(|c| {
        let source = c.get_source();
        let time = IntegerArgumentType::get_integer(c, "minutes");
        source.with_server(|server| server.set_player_idle_timeout(time));
        if time > 0 {
            source.send_success(|| translatable!("commands.setidletimeout.success", time), true);
        } else {
            source.send_success(|| translatable!("commands.setidletimeout.success.disabled"), true);
        }
        Ok(time)
    })));
});
