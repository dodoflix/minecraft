//! `JumpGoal` (abstract): the base only sets the `MOVE | JUMP` flags; concrete subclasses (`DolphinJumpGoal`, `Fox...`)
//! return [`JUMP_GOAL_FLAGS`] from `Goal::flags`.

use super::{Flag, GoalFlags};

/// `setFlags(EnumSet.of(Goal.Flag.MOVE, Goal.Flag.JUMP))`.
pub const JUMP_GOAL_FLAGS: GoalFlags = GoalFlags::of(&[Flag::Move, Flag::Jump]);
