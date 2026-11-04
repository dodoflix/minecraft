//! Scriptable headless Minecraft 26.3 client for integration/E2E tests.
pub mod bot;
pub mod conn;
pub mod error;
pub mod handlers;
pub mod harness;
pub mod inventory;
pub mod join;
pub mod log;
pub mod physics;
pub mod state;
pub mod tracking;
pub mod world;

pub use bot::Bot;
pub use error::{BotError, Result};
