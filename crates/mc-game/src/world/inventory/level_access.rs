//! `ContainerLevelAccess`: where a menu's block lives (`NULL` for menus not tied to a block).
use crate::server::level::ServerLevel;
use mc_core::pos::BlockPos;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerLevelAccess {
    Null,
    At { pos: BlockPos },
}

impl ContainerLevelAccess {
    /// `create(level, pos)` (the level is the one the player is in when the access is evaluated).
    pub fn create(pos: BlockPos) -> Self {
        Self::At { pos }
    }

    /// `evaluate(action)`: `None` for `NULL`.
    pub fn evaluate<T>(&self, level: &ServerLevel, action: impl FnOnce(&ServerLevel, BlockPos) -> T) -> Option<T> {
        match self {
            Self::Null => None,
            Self::At { pos } => Some(action(level, *pos)),
        }
    }

    /// `evaluate(action, defaultValue)`.
    pub fn evaluate_or<T>(&self, level: &ServerLevel, action: impl FnOnce(&ServerLevel, BlockPos) -> T, default: T) -> T {
        self.evaluate(level, action).unwrap_or(default)
    }
}
