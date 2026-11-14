//! `ServerGamePacketListenerImpl.handleDebugSubscriptionRequest`.

use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use linkme::distributed_slice;
use mc_protocol::packets::play::ServerboundDebugSubscriptionRequestPacket;

struct DebugSubscriptionHandlers;

impl PlayHandlers for DebugSubscriptionHandlers {
    fn handle_debug_subscription_request(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundDebugSubscriptionRequestPacket,
    ) {
        ctx.player
            .request_debug_subscriptions(&packet.subscriptions);
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static DEBUG_SUBSCRIPTION_HANDLERS: &dyn PlayHandlers = &DebugSubscriptionHandlers;
