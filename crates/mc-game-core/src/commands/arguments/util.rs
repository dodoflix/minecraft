//! Helpers shared by the argument parsers.

use mc_brigadier::{CommandExceptionType, CommandSyntaxException};
use mc_core::java_string::JavaString;
use mc_nbt::snbt::SnbtError;
use std::sync::LazyLock;

static SNBT_EXCEPTION_TYPE: LazyLock<CommandExceptionType> =
    LazyLock::new(CommandExceptionType::new);

/// The `CommandSyntaxException` a failed SNBT grammar parse raises.
pub fn snbt_exception(error: &SnbtError, input: &JavaString) -> CommandSyntaxException {
    CommandSyntaxException::with_context(
        *SNBT_EXCEPTION_TYPE,
        crate::commands::message::literal_message(error.message()),
        input.clone(),
        error.cursor(),
    )
}
