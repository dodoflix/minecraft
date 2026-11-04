//! `StopCommand`.
use crate::commands::{LEVEL_OWNERS, has_permission, literal};
use crate::{register_command, translatable};

register_command!("StopCommand", |ctx| {
    ctx.dispatcher.register(literal("stop").requires(has_permission(LEVEL_OWNERS)).executes(|c| {
        c.get_source().send_success(|| translatable!("commands.stop.stopping"), true);
        c.get_source().with_server(|server| server.shared.halt());
        Ok(1)
    }));
});

