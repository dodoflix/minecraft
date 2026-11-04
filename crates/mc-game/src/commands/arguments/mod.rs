//! `net.minecraft.commands.arguments`: every vanilla argument type. Each implements
//! `mc_brigadier::ArgumentType<CommandSourceStack>`; parsed values are read back with typed getters
//! (`XxxArgument::get_xxx(context, name)`).

use crate::commands::message::dynamic_translatable;
use mc_brigadier::DynamicCommandExceptionType;
use std::sync::LazyLock;

pub mod coordinates;
pub mod entity_anchor_argument;
mod simple_arguments;
mod string_representable_argument;

pub use simple_arguments::*;

/// `StringRepresentableArgument.ERROR_INVALID_VALUE`.
pub static ERROR_ENUM_INVALID: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.enum.invalid"));
