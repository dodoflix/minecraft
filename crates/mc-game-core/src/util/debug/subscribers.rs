//! `ServerDebugSubscribers`: the subscriptions at least one operator player has asked for, as of the end of the last
//! server tick, and the players that asked for each.

use crate::server::network::ConnectionSender;
use crate::server::players::PlayerHandle;
use mc_data::DebugSubscription;
use mc_protocol::packet::Packet;
use parking_lot::RwLock;
use std::sync::Arc;

/// The operators that asked for one subscription.
type Subscribed = (DebugSubscription, Vec<ConnectionSender>);

/// Shared by the server (which refreshes it at the end of every tick) and its levels. Sorted by subscription.
#[derive(Clone, Default)]
pub struct DebugSubscribers(Arc<RwLock<Vec<Subscribed>>>);

impl DebugSubscribers {
    /// `tick()`: the subscriptions the operators among `players` requested.
    pub fn tick(&self, players: &[PlayerHandle], is_op: impl Fn(&PlayerHandle) -> bool) {
        let mut enabled: Vec<Subscribed> = Vec::new();
        for player in players.iter().filter(|p| is_op(p)) {
            let requested = player.shared.lock().requested_debug_subscriptions.clone();
            for subscription in requested {
                match enabled.iter_mut().find(|(s, _)| *s == subscription) {
                    Some((_, subscribers)) => subscribers.push(player.connection.clone()),
                    None => enabled.push((subscription, vec![player.connection.clone()])),
                }
            }
        }
        enabled.sort_unstable_by_key(|(s, _)| *s);
        *self.0.write() = enabled;
    }

    /// `broadcastToAll(subscription, packet)`.
    pub fn broadcast_to_all<P: Packet>(&self, subscription: DebugSubscription, packet: &P) {
        for (s, subscribers) in self.0.read().iter() {
            if *s == subscription {
                for connection in subscribers {
                    connection.send(packet);
                }
            }
        }
    }

    /// `enabledSubscriptions()`.
    pub fn enabled(&self) -> Vec<DebugSubscription> {
        self.0.read().iter().map(|(s, _)| *s).collect()
    }

    /// `hasAnySubscriberFor(subscription)`.
    pub fn has_any_subscriber_for(&self, subscription: DebugSubscription) -> bool {
        self.0.read().iter().any(|(s, _)| *s == subscription)
    }
}
