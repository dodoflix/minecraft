//! The read side of a connection in the play protocol: decodes serverbound play packets and forwards them to the
//! play listener on the game thread (`InboundPacket`), ending with one `Disconnected` event — or, after
//! `ServerboundConfigurationAcknowledgedPacket` (a terminal packet: vanilla stops reading until the inbound protocol is
//! switched), with the cookie of the next configuration phase.

use super::common_listener::CommonListenerCookie;
use super::connection::{DisconnectionDetails, ReadError, get_millis};
use super::connection_task::{ConnectionContext, Event};
use super::pending_player::InboundPacket;
use mc_protocol::codec::read_varint;
use mc_protocol::ids;
use mc_protocol::net::NetError;
use mc_protocol::packets::play::ServerboundPlayPacket;
use mc_text::Component;
use std::sync::mpsc::Sender;
use tokio::sync::mpsc::unbounded_channel;

pub(super) async fn run(ctx: &mut ConnectionContext, inbound: Sender<InboundPacket>) -> Option<CommonListenerCookie> {
    let (switch_tx, mut switch_rx) = unbounded_channel();
    ctx.handle.set_reconfigure_sender(switch_tx);
    loop {
        match ctx.next_event().await {
            Event::Tick => {}
            Event::ReadError(e) => ctx.fault(e),
            Event::Payload(p) => {
                let received_at_ms = get_millis();
                let terminal = read_varint(&mut &p[..]).is_ok_and(|id| id == ids::play::serverbound::CONFIGURATION_ACKNOWLEDGED);
                match ServerboundPlayPacket::decode_payload(&p) {
                    Err(e) => ctx.fault(ReadError::Net(NetError::Decode(e))),
                    Ok(packet) => {
                        if inbound.send(InboundPacket::Packet { packet: Box::new(packet), received_at_ms }).is_err() {
                            // The play listener is gone (player removed): nobody reads this connection any more.
                            ctx.sender.close();
                        } else if terminal {
                            // Not another byte is decoded until the game thread switched the protocol.
                            tokio::select! {
                                cookie = switch_rx.recv() => return cookie,
                                () = ctx.handle.stopped() => {}
                            }
                        }
                    }
                }
            }
            Event::Stopped => {
                ctx.wait_closed().await;
                let details = ctx
                    .handle
                    .disconnection_details()
                    .unwrap_or_else(|| DisconnectionDetails::new(Component::translatable("multiplayer.disconnect.generic", vec![])));
                let _ = inbound.send(InboundPacket::Disconnected(details));
                return None;
            }
        }
    }
}
