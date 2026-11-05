//! `net.minecraft.commands.CommandSource`: where a command's feedback goes.

use mc_text::Component;
use std::sync::Arc;
use uuid::Uuid;

/// What a [`CommandSource`] is, where vanilla compares source identities (`source != server`,
/// `player.commandSource() != source`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandSourceKind {
    Other,
    /// The `MinecraftServer` itself (console).
    Server,
    /// A `ServerPlayer` (its `commandSource()`).
    Player(Uuid),
}

pub trait CommandSource: Send + Sync {
    fn send_system_message(&self, message: &Component);
    fn accepts_success(&self) -> bool;
    fn accepts_failure(&self) -> bool;
    fn should_inform_admins(&self) -> bool;
    fn always_accepts(&self) -> bool {
        false
    }
    fn kind(&self) -> CommandSourceKind {
        CommandSourceKind::Other
    }
}

/// `CommandSource.NULL`.
pub struct NullCommandSource;

impl CommandSource for NullCommandSource {
    fn send_system_message(&self, _message: &Component) {}
    fn accepts_success(&self) -> bool {
        false
    }
    fn accepts_failure(&self) -> bool {
        false
    }
    fn should_inform_admins(&self) -> bool {
        false
    }
}

/// `CommandSource.NULL` as a shared handle.
pub fn null_source() -> Arc<dyn CommandSource> {
    static NULL: std::sync::LazyLock<Arc<dyn CommandSource>> = std::sync::LazyLock::new(|| Arc::new(NullCommandSource));
    NULL.clone()
}
