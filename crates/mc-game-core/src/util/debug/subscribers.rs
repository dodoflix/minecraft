//! `ServerDebugSubscribers`: the subscriptions at least one operator player has asked for, as of the end of the last
//! server tick.

use crate::server::players::PlayerHandle;
use mc_data::DebugSubscription;
use parking_lot::RwLock;
use std::sync::Arc;

/// Shared by the server (which refreshes it at the end of every tick) and its levels.
#[derive(Clone, Default)]
pub struct DebugSubscribers(Arc<RwLock<Vec<DebugSubscription>>>);

impl DebugSubscribers {
    /// `tick()`: the subscriptions the operators among `players` requested.
    pub fn tick(&self, players: &[PlayerHandle], is_op: impl Fn(&PlayerHandle) -> bool) {
        let mut enabled: Vec<DebugSubscription> = players
            .iter()
            .filter(|p| is_op(p))
            .flat_map(|p| p.shared.lock().requested_debug_subscriptions.clone())
            .collect();
        enabled.sort_unstable();
        enabled.dedup();
        *self.0.write() = enabled;
    }

    /// `enabledSubscriptions()`.
    pub fn enabled(&self) -> Vec<DebugSubscription> {
        self.0.read().clone()
    }

    /// `hasAnySubscriberFor(subscription)`.
    pub fn has_any_subscriber_for(&self, subscription: DebugSubscription) -> bool {
        self.0.read().contains(&subscription)
    }
}
