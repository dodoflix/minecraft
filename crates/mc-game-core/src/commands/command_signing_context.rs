//! `net.minecraft.commands.CommandSigningContext`.

use crate::server::chat::PlayerChatMessage;
use std::collections::HashMap;
use std::sync::Arc;

/// Signed chat arguments of a command (`name -> PlayerChatMessage`).
#[derive(Clone, Default)]
pub enum CommandSigningContext {
    /// `CommandSigningContext.ANONYMOUS`.
    #[default]
    Anonymous,
    /// `CommandSigningContext.SignedArguments`.
    Signed(Arc<HashMap<String, PlayerChatMessage>>),
}

impl CommandSigningContext {
    pub fn get_argument(&self, name: &str) -> Option<&PlayerChatMessage> {
        match self {
            Self::Anonymous => None,
            Self::Signed(arguments) => arguments.get(name),
        }
    }
}
