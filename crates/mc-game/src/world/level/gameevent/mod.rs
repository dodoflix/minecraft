//! `net.minecraft.world.level.gameevent`: game event context, listener contract, position sources and the
//! per-section listener registry. The dispatcher (`GameEventDispatcher`) is `server::level::game_event_dispatcher`.
//!
//! Listeners are owned by block entities / entities (vanilla: `EntityBlock.getListener`, `DynamicGameEventListener`),
//! which live in take-out storage, so the registries hold *handles* ([`ListenerOwner`]) plus the listener's source
//! and radius sampled at registration; delivery resolves the owner and calls its [`GameEventListener`].

mod listener;
mod registry;
pub mod vibrations;

pub use listener::*;
pub use registry::*;
