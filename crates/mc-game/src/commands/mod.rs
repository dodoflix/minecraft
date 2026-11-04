//! `net.minecraft.commands`: the command framework (dispatcher, source stack, execution engine, argument types,
//! synchronization with the client) and, in `server::commands`, the commands themselves.

pub mod arguments;
pub mod brigadier_exceptions;
pub mod command_build_context;
pub mod command_result_callback;
pub mod command_signing_context;
pub mod command_source;
pub mod command_source_stack;
pub mod commands;
pub mod execution;
pub mod functions;
pub mod host;
pub mod message;
pub mod server_access;
pub mod shared_suggestion_provider;

pub use brigadier_exceptions::BrigadierExceptions;
pub use command_build_context::{CommandBuildContext, HolderLookupProvider, RegistryView};
pub use command_result_callback::CommandResultCallback;
pub use command_signing_context::CommandSigningContext;
pub use command_source::{CommandSource, CommandSourceKind, NullCommandSource, null_source};
pub use command_source_stack::{CommandEntity, CommandSourceStack};
pub use commands::{
    COMMANDS, COMMAND_PREFIX, CommandRegistration, CommandRegistrationContext, CommandSelection, Commands, LEVEL_ADMINS, LEVEL_ALL,
    LEVEL_GAMEMASTERS, LEVEL_MODERATORS, LEVEL_OWNERS, argument, has_permission, literal,
};

use mc_brigadier::ResultConsumer;
use std::sync::Arc;

/// `ExecutionCommandSource.resultConsumer()`.
pub fn execution_command_source_result_consumer() -> ResultConsumer<CommandSourceStack> {
    Arc::new(|context, success, result| context.get_source().callback().on_result(success, result))
}
