//! `ServerCommonPacketListenerImpl`: the state and logic shared by the configuration and play listeners
//! (keep-alive, latency, the closed-listener timeout, `disconnect`). Plain `Send` data without server references:
//! the configuration listener runs inside the connection's tokio task, so time is passed in as `now_ms`
//! (`Util.getMillis()`) and the connection through the [`ConnectionSender`].

use mc_protocol::codec::GameProfile;
use mc_protocol::packets::common::ClientInformation;
use mc_text::Component;

/// `ServerCommonPacketListenerImpl.LATENCY_CHECK_INTERVAL` (ms).
pub const LATENCY_CHECK_INTERVAL: i64 = 15000;
/// `CLOSED_LISTENER_TIMEOUT` (ms).
const CLOSED_LISTENER_TIMEOUT: i64 = 15000;

/// `CommonListenerCookie`: what carries over between the configuration and play listeners.
#[derive(Clone, Debug)]
pub struct CommonListenerCookie {
    pub profile: GameProfile,
    pub latency: i32,
    pub client_information: ClientInformation,
    pub transferred: bool,
}

/// `TIMEOUT_DISCONNECTION_MESSAGE`.
pub fn timeout_disconnection_message() -> Component {
    Component::translatable("disconnect.timeout", vec![])
}

/// `DISCONNECT_UNEXPECTED_QUERY`.
pub fn disconnect_unexpected_query() -> Component {
    Component::translatable("multiplayer.disconnect.unexpected_query_response", vec![])
}

/// What [`ServerCommonListener::poll_keep_alive`] decided.
#[derive(Debug, PartialEq, Eq)]
pub enum KeepAliveAction {
    Nothing,
    /// Send `ClientboundKeepAlivePacket(challenge)`.
    Send(i64),
    /// Disconnect with `disconnect.timeout`.
    Timeout,
}

pub struct ServerCommonListener {
    transferred: bool,
    keep_alive_time: i64,
    keep_alive_pending: bool,
    keep_alive_challenge: i64,
    closed_listener_time: i64,
    closed: bool,
    latency: i32,
    /// `isSingleplayerOwner()`: always false on a dedicated server (no singleplayer profile).
    singleplayer_owner: bool,
}

impl ServerCommonListener {
    pub fn new(cookie: &CommonListenerCookie, now_ms: i64) -> Self {
        Self {
            transferred: cookie.transferred,
            keep_alive_time: now_ms,
            keep_alive_pending: false,
            keep_alive_challenge: 0,
            closed_listener_time: 0,
            closed: false,
            latency: cookie.latency,
            singleplayer_owner: false,
        }
    }

    pub fn latency(&self) -> i32 {
        self.latency
    }

    pub fn transferred(&self) -> bool {
        self.transferred
    }

    /// `close()` (called when a terminal packet, e.g. `ClientboundFinishConfigurationPacket`, is sent).
    pub fn close(&mut self, now_ms: i64) {
        if !self.closed {
            self.closed_listener_time = now_ms;
            self.closed = true;
        }
    }

    /// `handleKeepAlive(packet.getId())` (the packet types differ per state, the logic is shared).
    /// Returns `true` when the caller must disconnect with [`timeout_disconnection_message`] (unexpected/late response).
    pub fn handle_keep_alive(&mut self, id: i64, now_ms: i64) -> bool {
        if self.keep_alive_pending && id == self.keep_alive_challenge {
            // `(int)(Util.getMillis() - keepAliveTime)`
            let time = now_ms.wrapping_sub(self.keep_alive_time) as i32;
            self.latency = self.latency.wrapping_mul(3).wrapping_add(time) / 4;
            self.keep_alive_pending = false;
            false
        } else {
            !self.singleplayer_owner
        }
    }

    /// `keepConnectionAlive()`: decides what to do; the caller sends its state's `ClientboundKeepAlivePacket(id)` or
    /// disconnects with `timeout_disconnection_message()`.
    pub fn poll_keep_alive(&mut self, now_ms: i64) -> KeepAliveAction {
        if !self.singleplayer_owner && now_ms - self.keep_alive_time >= LATENCY_CHECK_INTERVAL {
            if self.keep_alive_pending {
                return KeepAliveAction::Timeout;
            }
            if self.check_if_closed(now_ms) {
                self.keep_alive_pending = true;
                self.keep_alive_time = now_ms;
                self.keep_alive_challenge = now_ms;
                return KeepAliveAction::Send(self.keep_alive_challenge);
            }
            if self.closed && now_ms - self.closed_listener_time >= CLOSED_LISTENER_TIMEOUT {
                return KeepAliveAction::Timeout;
            }
        }
        KeepAliveAction::Nothing
    }

    /// `checkIfClosed(now)` minus its disconnect (see `poll_keep_alive`).
    fn check_if_closed(&self, _now_ms: i64) -> bool {
        !self.closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(now: i64) -> ServerCommonListener {
        ServerCommonListener::new(
            &CommonListenerCookie {
                profile: GameProfile {
                    id: uuid::Uuid::nil(),
                    name: "t".into(),
                    properties: vec![],
                },
                latency: 0,
                client_information: ClientInformation::default(),
                transferred: false,
            },
            now,
        )
    }

    #[test]
    fn keep_alive_sent_every_15s_then_times_out() {
        let mut l = listener(1000);
        assert_eq!(l.poll_keep_alive(15999), KeepAliveAction::Nothing);
        assert_eq!(l.poll_keep_alive(16000), KeepAliveAction::Send(16000));
        // unanswered: next poll after another 15 s kicks
        assert_eq!(l.poll_keep_alive(30999), KeepAliveAction::Nothing);
        assert_eq!(l.poll_keep_alive(31000), KeepAliveAction::Timeout);
    }

    #[test]
    fn latency_is_smoothed_and_wrong_challenge_is_a_timeout_path() {
        let mut l = listener(0);
        assert_eq!(l.poll_keep_alive(15000), KeepAliveAction::Send(15000));
        // answered 100 ms later: (0*3 + 100) / 4
        l.latency = 40;
        assert!(!l.handle_keep_alive(15000, 15100));
        assert_eq!(l.latency(), (40 * 3 + 100) / 4);
        assert!(!l.keep_alive_pending);
        // a second answer (or a wrong challenge) is a timeout kick
        assert!(l.handle_keep_alive(15000, 15200));
    }
}
