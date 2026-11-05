//! `JumpGoal` (abstract): the base only sets the `MOVE | JUMP` flags; concrete subclasses (`DolphinJumpGoal`, `Fox...`)
//! return [`JUMP_GOAL_FLAGS`] from `Goal::flags`.

use mc_game_core::world::entity::ai::goal::{Flag, GoalFlags};

/// `setFlags(EnumSet.of(Goal.Flag.MOVE, Goal.Flag.JUMP))`.
pub const JUMP_GOAL_FLAGS: GoalFlags = GoalFlags::of(&[Flag::Move, Flag::Jump]);
