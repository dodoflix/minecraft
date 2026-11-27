//! Helpers shared by the argument parsers.

use mc_brigadier::{CommandExceptionType, CommandSyntaxException};
use mc_core::java_string::JavaString;
use mc_nbt::snbt::SnbtError;
use std::sync::LazyLock;

static SNBT_EXCEPTION_TYPE: LazyLock<CommandExceptionType> =
    LazyLock::new(CommandExceptionType::new);

/// The `CommandSyntaxException` a failed SNBT grammar parse raises.
pub fn snbt_exception(error: &SnbtError, input: &JavaString) -> CommandSyntaxException {
    use crate::commands::message::{literal_message, message};
    let msg = match error.translation() {
        Some((key, arg)) => message(mc_text::Component::translatable(
            key,
            arg.map(|a| mc_text::component::TranslatableArg::from(a.to_string()))
                .into_iter()
                .collect(),
        )),
        None => literal_message(error.message()),
    };
    CommandSyntaxException::with_context(*SNBT_EXCEPTION_TYPE, msg, input.clone(), error.cursor())
}
