//! `net.minecraft.commands.arguments`: every vanilla argument type. Each implements
//! `mc_brigadier::ArgumentType<CommandSourceStack>`; parsed values are read back with typed getters
//! (`XxxArgument::get_xxx(context, name)`).

use crate::commands::message::dynamic_translatable;
use mc_brigadier::DynamicCommandExceptionType;
use std::sync::LazyLock;

pub mod blocks;
pub mod coordinates;
pub mod entity_anchor_argument;
pub mod entity_argument;
pub mod function_argument;
pub mod game_profile_argument;
pub mod item;
pub mod message_argument;
pub mod min_max_bounds;
pub mod nbt_arguments;
pub mod nbt_path_argument;
pub mod operation_argument;
pub mod range_argument;
pub mod resource_arguments;
pub mod scores;
pub mod slot_arguments;
pub mod selector;
mod simple_arguments;
mod string_representable_argument;
pub mod util;

pub use entity_argument::EntityArgument;
pub use game_profile_argument::GameProfileArgument;
pub use message_argument::MessageArgument;
pub use nbt_arguments::{ComponentArgument, CompoundTagArgument, NbtTagArgument, StyleArgument};
pub use nbt_path_argument::{NbtPath, NbtPathArgument};
pub use operation_argument::{Operation, OperationArgument};
pub use range_argument::{FloatRangeArgument, IntRangeArgument};
pub use scores::{ObjectiveArgument, ObjectiveCriteriaArgument, ScoreHolderArgument, ScoreboardSlotArgument, TeamArgument};
pub use simple_arguments::*;

/// `StringRepresentableArgument.ERROR_INVALID_VALUE`.
pub static ERROR_ENUM_INVALID: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.enum.invalid"));
