//! `net.minecraft.commands.CommandSigningContext`.

use crate::server::chat::PlayerChatMessage;
use crate::server::network::future_chain::FutureChain;
use crate::server::network::text_filter::TextFilter;
use std::collections::HashMap;
use std::sync::Arc;

/// What a command a player sent signed needs to filter and order its chat arguments: `CommandSourceStack.getChatMessageChainer()`
/// and the sender's `getTextFilter()`.
#[derive(Clone)]
pub struct ChatServices {
    pub filter: Arc<dyn TextFilter>,
    pub chain: Arc<FutureChain>,
}

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
