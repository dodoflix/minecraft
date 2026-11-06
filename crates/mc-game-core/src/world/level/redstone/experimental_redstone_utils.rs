//! `net.minecraft.world.level.redstone.ExperimentalRedstoneUtils`.
use super::Orientation;
use crate::server::level::ServerLevel;
use mc_core::direction::Direction;

pub struct ExperimentalRedstoneUtils;

impl ExperimentalRedstoneUtils {
    /// `initialOrientation(level, front, up)`: `None` unless `REDSTONE_EXPERIMENTS` is enabled (then one `nextInt(48)`).
    pub fn initial_orientation(
        level: &mut ServerLevel,
        front: Option<Direction>,
        up: Option<Direction>,
    ) -> Option<Orientation> {
        level.initial_orientation(front, up)
    }

    /// `withFront(orientation, front)`.
    pub fn with_front(orientation: Option<Orientation>, front: Direction) -> Option<Orientation> {
        orientation.map(|o| o.with_front(front))
    }
}
