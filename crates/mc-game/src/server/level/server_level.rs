//! `ServerLevel`. Skeleton: the LevelAgent slice grows this struct (chunk source, tickets, entity storage, ticks).
use crate::world::level::DimensionKey;

pub struct ServerLevel {
    pub dimension: DimensionKey,
}
