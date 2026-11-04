//! Scriptable headless Minecraft 26.3 client for integration/E2E tests.
pub mod conn;
pub mod error;
pub mod harness;
pub mod join;
pub mod log;
pub mod physics;
pub mod world;

pub use error::{BotError, Result};
