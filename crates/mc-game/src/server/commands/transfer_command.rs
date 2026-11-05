//! `TransferCommand`.
use crate::commands::message::simple_translatable;
use crate::commands::arguments::EntityArgument;
use crate::commands::arguments::selector::EntityRef;
use crate::commands::{CommandSourceStack, LEVEL_ADMINS, argument, has_permission, host, literal};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{IntegerArgumentType, StringArgumentType, integer_range};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, SimpleCommandExceptionType};
use mc_protocol::packets::play::ClientboundTransferPacket;
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_NO_PLAYERS: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.transfer.error.no_players"));

register_command!("TransferCommand", |ctx| {
    ctx.dispatcher.register(
        literal("transfer").requires(has_permission(LEVEL_ADMINS)).then(
            argument("hostname", StringArgumentType::string())
                .executes(|c| {
                    let player = c.get_source().get_player_or_exception()?;
                    let players = vec![EntityRef { dimension: c.get_source().get_level().clone(), id: player.id }];
                    transfer(c.get_source(), &StringArgumentType::get_string(c, "hostname"), 25565, players)
                })
                .then(
                    argument("port", integer_range(1, 65535))
                        .executes(|c| {
                            let player = c.get_source().get_player_or_exception()?;
                            let players = vec![EntityRef { dimension: c.get_source().get_level().clone(), id: player.id }];
                            transfer(c.get_source(), &StringArgumentType::get_string(c, "hostname"), IntegerArgumentType::get_integer(c, "port"), players)
                        })
                        .then(argument("players", EntityArgument::players()).executes(|c| {
                            transfer(
                                c.get_source(),
                                &StringArgumentType::get_string(c, "hostname"),
                                IntegerArgumentType::get_integer(c, "port"),
                                EntityArgument::get_players(c, "players")?,
                            )
                        })),
                ),
        ),
    );
});

/// `CommandResponseTracker` with every player tracked with value 1; `onZero` = `commands.transfer.error.no_players`.
fn transfer(source: &CommandSourceStack, hostname: &str, port: i32, players: Vec<EntityRef>) -> Result<i32, CommandSyntaxException> {
    for player in &players {
        if let Some(handle) = source.with_server(|server| host::player_handle(server, player)) {
            handle.connection.send(&ClientboundTransferPacket { host: hostname.to_owned(), port });
        }
    }
    let total = players.len() as i32;
    if total == 0 {
        return Err(ERROR_NO_PLAYERS.create());
    }
    source.send_success(
        || match players.as_slice() {
            [only] => {
                let name = source.with_server(|server| host::get_entity(server, only).map_or_else(Component::empty, |e| host::entity_display_name(server, e)));
                translatable!("commands.transfer.success.single", name, hostname, port)
            }
            _ => translatable!("commands.transfer.success.multiple", total, hostname, port),
        },
        true,
    );
    Ok(total)
}
