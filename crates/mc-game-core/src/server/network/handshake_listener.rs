//! `ServerHandshakePacketListenerImpl`.

use super::connection_task::ConnectionContext;
use super::hooks::{PROTOCOL_VERSION, VERSION_NAME};
use mc_protocol::codec::JsonComponent;
use mc_protocol::packet::ConnectionState;
use mc_protocol::packets::handshake::{ClientIntent, ClientIntentionPacket};
use mc_protocol::packets::login::ClientboundLoginDisconnectPacket;
use mc_protocol::packets::status::ServerStatus;
use mc_text::{Component, TranslatableArg};
use std::sync::Arc;

pub(super) enum HandshakeOutcome {
    /// The connection was refused (and closed).
    Closed,
    /// Status protocol with this status.
    Status(Arc<ServerStatus>),
    Login {
        transferred: bool,
    },
}

/// `handleIntention`.
pub(super) async fn handle_intention(
    ctx: &mut ConnectionContext,
    packet: ClientIntentionPacket,
) -> HandshakeOutcome {
    let settings = ctx.hooks.settings();
    match packet.intention {
        ClientIntent::Login => begin_login(ctx, &packet, false),
        ClientIntent::Status => {
            let status = ctx.hooks.status();
            ctx.sender.set_outbound_state(ConnectionState::Status);
            match status {
                Some(status) if settings.replies_to_status => HandshakeOutcome::Status(status),
                _ => {
                    ctx.sender.close_with(Component::translatable(
                        "disconnect.ignoring_status_request",
                        vec![],
                    ));
                    HandshakeOutcome::Closed
                }
            }
        }
        ClientIntent::Transfer => {
            if !settings.accepts_transfers {
                ctx.sender.set_outbound_state(ConnectionState::Login);
                let reason =
                    Component::translatable("multiplayer.disconnect.transfers_disabled", vec![]);
                ctx.sender.send(&ClientboundLoginDisconnectPacket {
                    reason: JsonComponent(reason.clone()),
                });
                ctx.sender.close_with(reason);
                HandshakeOutcome::Closed
            } else {
                begin_login(ctx, &packet, true)
            }
        }
    }
}

fn begin_login(
    ctx: &mut ConnectionContext,
    packet: &ClientIntentionPacket,
    transferred: bool,
) -> HandshakeOutcome {
    ctx.sender.set_outbound_state(ConnectionState::Login);
    if packet.protocol_version != PROTOCOL_VERSION {
        let key = if packet.protocol_version < 754 {
            "multiplayer.disconnect.outdated_client"
        } else {
            "multiplayer.disconnect.incompatible"
        };
        let reason =
            Component::translatable(key, vec![TranslatableArg::String(VERSION_NAME.to_owned())]);
        ctx.sender.send(&ClientboundLoginDisconnectPacket {
            reason: JsonComponent(reason.clone()),
        });
        ctx.sender.close_with(reason);
        HandshakeOutcome::Closed
    } else {
        HandshakeOutcome::Login { transferred }
    }
}
