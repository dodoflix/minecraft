//! `net.minecraft.util.debug`: debug subscriptions. A client with operator permissions asks for subscriptions
//! (`ServerboundDebugSubscriptionRequestPacket`); the level's [`LevelDebugSynchronizers`] then sends it the values the
//! entities, chunks and block entities register (`DebugValueSource.registerDebugValues`) whenever they change.

pub mod brain_dump;
mod chunk_values;
pub mod entity_name;
mod play_handler;
mod subscribers;
pub mod subscriptions;
mod synchronizers;

pub use subscribers::DebugSubscribers;
pub use synchronizers::{
    BlockEntityDebugGetter, ChunkDebugGetter, DebugRegistration, DebugValue, EntityDebugGetter,
    LevelDebugSynchronizers, TrackerEvent,
};
