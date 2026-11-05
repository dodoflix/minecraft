//! `ContainerLevelAccess`: where a menu's block lives (`NULL` for menus not tied to a block).
use super::MenuPlayer;
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use mc_core::pos::BlockPos;
use mc_data::Block;

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

    /// The access position (`None` for `NULL`): pair it with `MenuPlayer::level()` for `execute` side effects.
    pub fn pos(&self) -> Option<BlockPos> {
        match self {
            Self::Null => None,
            Self::At { pos } => Some(*pos),
        }
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

/// `AbstractContainerMenu.stillValid(access, player, block)`: `NULL` access is always valid; a menu without a level
/// (test player) cannot evaluate the action and is treated as `NULL`.
pub fn still_valid(access: &ContainerLevelAccess, player: &dyn MenuPlayer, block: Block) -> bool {
    match (access.pos(), player.level_ref()) {
        (Some(pos), Some(level)) => level.get_block_state(pos).block() == block && player.is_within_block_interaction_range(pos, 4.0),
        _ => true,
    }
}
