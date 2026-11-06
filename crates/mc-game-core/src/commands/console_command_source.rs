//! `CommandSource` implementations of the server console and RCON (`MinecraftServer implements CommandSource`,
//! `RconConsoleSource`).

use super::{CommandSource, CommandSourceKind};
use mc_text::Component;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The dedicated server as a command source: feedback goes to the log (`MinecraftServer.sendSystemMessage`).
pub struct ConsoleCommandSource;

impl CommandSource for ConsoleCommandSource {
    fn send_system_message(&self, message: &Component) {
        log::info!("System chat: {}", message.get_string());
    }

    fn accepts_success(&self) -> bool {
        true
    }

    fn accepts_failure(&self) -> bool {
        true
    }

    /// `DedicatedServer.shouldInformAdmins`.
    fn should_inform_admins(&self) -> bool {
        true
    }

    fn kind(&self) -> CommandSourceKind {
        CommandSourceKind::Server
    }
}

/// `RconConsoleSource`: collects the feedback of one command in a buffer that is sent back to the RCON client.
pub struct RconConsoleSource {
    buffer: Mutex<String>,
    /// `MinecraftServer.shouldRconBroadcast()` (`broadcast-rcon-to-ops`).
    broadcast_to_ops: Arc<AtomicBool>,
}

impl RconConsoleSource {
    pub fn new(broadcast_to_ops: Arc<AtomicBool>) -> Self {
        Self {
            buffer: Mutex::new(String::new()),
            broadcast_to_ops,
        }
    }

    pub fn prepare_for_command(&self) {
        self.buffer.lock().clear();
    }

    pub fn get_command_response(&self) -> String {
        self.buffer.lock().clone()
    }

    /// The `Rcon` display name of its stack.
    pub fn name() -> Component {
        Component::literal("Rcon")
    }
}

impl CommandSource for RconConsoleSource {
    fn send_system_message(&self, message: &Component) {
        self.buffer.lock().push_str(&message.get_string());
    }

    fn accepts_success(&self) -> bool {
        true
    }

    fn accepts_failure(&self) -> bool {
        true
    }

    fn should_inform_admins(&self) -> bool {
        self.broadcast_to_ops.load(Ordering::Relaxed)
    }
}
