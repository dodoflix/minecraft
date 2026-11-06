//! `ServerStatusPacketListenerImpl`.

use super::connection::ReadError;
use super::connection_task::{ConnectionContext, Event};
use mc_protocol::net::NetError;
use mc_protocol::packets::status::{
    ClientboundPongResponsePacket, ClientboundStatusResponsePacket, ServerStatus,
    ServerboundStatusPacket,
};
use mc_text::Component;
use std::sync::Arc;

fn disconnect_reason() -> Component {
    Component::translatable("multiplayer.status.request_handled", vec![])
}

pub(super) async fn run(ctx: &mut ConnectionContext, status: Arc<ServerStatus>) {
    let mut has_requested_status = false;
    loop {
        match ctx.next_event().await {
            Event::Tick => {}
            Event::Stopped => return,
            Event::ReadError(e) => ctx.fault(e),
            Event::Payload(p) => match ServerboundStatusPacket::decode_payload(&p) {
                Err(e) => ctx.fault(ReadError::Net(NetError::Decode(e))),
                Ok(ServerboundStatusPacket::StatusRequest(_)) => {
                    if has_requested_status {
                        ctx.sender.close_with(disconnect_reason());
                    } else {
                        has_requested_status = true;
                        ctx.sender.send(&ClientboundStatusResponsePacket {
                            status: (*status).clone(),
                        });
                    }
                }
                Ok(ServerboundStatusPacket::PingRequest(ping)) => {
                    ctx.sender
                        .send(&ClientboundPongResponsePacket { time: ping.time });
                    ctx.sender.close_with(disconnect_reason());
                }
            },
        }
    }
}
