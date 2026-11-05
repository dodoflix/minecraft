//! Running commands from code that executes inside a level's own tick (command blocks): vanilla calls
//! `server.getCommands().performPrefixedCommand(source, command)` synchronously from `CommandBlock.tick`, but here the
//! ticking level is taken out of the server and block code only has the level. While [`ServerLevel::tick`] runs the
//! scheduled block ticks the tick environment (which owns the server borrow) is reachable through the level
//! ([`ServerLevel::with_command_env`]); [`ServerLevel::perform_prefixed_command`] hands the command to it, which lends this
//! very level back to the server for the duration of the command (`MinecraftServer::with_lent_level`).

use super::ServerLevel;
use super::level_tick::LevelTickEnv;
use crate::commands::CommandSourceStack;
use std::panic::{self, AssertUnwindSafe};
use std::ptr::NonNull;

/// The game rules command blocks read, live from the server (`level.getGameRules()` in vanilla).
#[derive(Clone, Copy, Debug)]
pub struct CommandRules {
    /// `COMMAND_BLOCKS_WORK` (`ServerLevel.isCommandBlockEnabled()`).
    pub command_blocks_work: bool,
    /// `COMMAND_BLOCK_OUTPUT`.
    pub command_block_output: bool,
    /// `SEND_COMMAND_FEEDBACK`.
    pub send_command_feedback: bool,
    /// `MAX_COMMAND_SEQUENCE_LENGTH`.
    pub max_command_sequence_length: i32,
}

impl Default for CommandRules {
    /// The vanilla defaults.
    fn default() -> Self {
        Self { command_blocks_work: true, command_block_output: true, send_command_feedback: true, max_command_sequence_length: 65536 }
    }
}

/// The tick environment of the running tick with its lifetime erased (valid only inside `with_command_env`).
pub struct CommandEnvPtr(NonNull<dyn LevelTickEnv>);

impl ServerLevel {
    /// Makes `env` reachable through [`ServerLevel::perform_prefixed_command`] while `f` runs. `f` must not use `env` itself
    /// (the caller hands over its only borrow).
    pub(crate) fn with_command_env<R>(&mut self, env: &mut dyn LevelTickEnv, f: impl FnOnce(&mut ServerLevel) -> R) -> R {
        // SAFETY: only the lifetime is erased; the pointer is removed again before `env`'s borrow ends (also on unwind) and
        // `perform_prefixed_command` is the only user while `f` has the sole use of the borrow.
        let erased: &'static mut dyn LevelTickEnv = unsafe { std::mem::transmute::<&mut dyn LevelTickEnv, &'static mut dyn LevelTickEnv>(env) };
        let previous = self.tick_state.command_env.replace(CommandEnvPtr(NonNull::from(erased)));
        let result = panic::catch_unwind(AssertUnwindSafe(|| f(self)));
        self.tick_state.command_env = previous;
        match result {
            Ok(r) => r,
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    /// The command block game rules: live from the server while this level runs its scheduled block ticks, otherwise the
    /// snapshot of the start of the level's last tick.
    pub fn command_rules(&self) -> CommandRules {
        match &self.tick_state.command_env {
            // SAFETY: see `with_command_env`; a shared read of the environment.
            Some(env) => unsafe { env.0.as_ref() }.command_rules(),
            None => self.tick_state.command_rules,
        }
    }

    /// Whether a command can be run from this level right now (it is being ticked by a server).
    pub fn can_perform_commands(&self) -> bool {
        self.tick_state.command_env.is_some()
    }

    /// `server.getCommands().performPrefixedCommand(source, command)` for a source located in this level. Panics when
    /// this level is not inside a server's tick (levels of tests without a server cannot run commands).
    pub fn perform_prefixed_command(&mut self, source: CommandSourceStack, command: &str) {
        let mut env = self.tick_state.command_env.take().expect("commands can only run while a server ticks the level");
        // SAFETY: see `with_command_env`: the pointer was made from the live `&mut` of the tick environment and was taken out
        // above, so this is the only access to it until it is put back.
        unsafe { env.0.as_mut() }.perform_prefixed_command(self, source, command);
        self.tick_state.command_env = Some(env);
    }
}
