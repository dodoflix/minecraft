//! `net.minecraft.server.network`: connection listener + handshake/status/login/configuration/game packet listeners.
pub mod connection;
pub use connection::{ConnectionSender, Outbound};
