//! `net.minecraft.commands.CommandSigningContext`.

use std::collections::HashMap;
use std::sync::Arc;

/// Signed chat arguments of a command (`name -> PlayerChatMessage`). The chat slice owns `PlayerChatMessage`; until it
/// lands the signed messages are kept as opaque shared values the chat code downcasts.
#[derive(Clone, Default)]
pub enum CommandSigningContext {
    /// `CommandSigningContext.ANONYMOUS`.
    #[default]
    Anonymous,
    Signed(Arc<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>),
}

impl CommandSigningContext {
    pub fn get_argument(&self, name: &str) -> Option<&Arc<dyn std::any::Any + Send + Sync>> {
        match self {
            Self::Anonymous => None,
            Self::Signed(arguments) => arguments.get(name),
        }
    }
}
