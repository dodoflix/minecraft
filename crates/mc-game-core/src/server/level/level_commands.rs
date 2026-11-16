//! Running commands from code that executes inside a level's own tick (command blocks): vanilla calls
//! `server.getCommands().performPrefixedCommand(source, command)` synchronously from `CommandBlock.tick`, but here the
//! ticking level is taken out of the server and block code only has the level. While [`ServerLevel::tick`] runs the
//! scheduled block ticks the tick environment (which owns the server borrow) is reachable through the level
//! (`ServerLevel::with_command_env`); [`ServerLevel::perform_prefixed_command`] hands the command to it, which lends this
//! very level back to the server for the duration of the command (`MinecraftServer::with_lent_level`).

use super::ServerLevel;
use super::level_tick::LevelTickEnv;
use crate::commands::CommandSourceStack;
use crate::world::entity::EntityId;
use mc_core::ident::Identifier;
use mc_core::phys::{Vec2, Vec3};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::NonNull;

/// An advancement reward function (`AdvancementRewards.function`). Vanilla runs it inside `grant`, but the awarded player is
/// out of its level then and the commands could not select it (`@s`): the function waits in the level's queue until the player
/// is back and the server can run it ([`MinecraftServer::run_pending_functions`](crate::server::MinecraftServer)). The
/// player's position and rotation are those of the award (`player.createCommandSourceStack()`).
pub struct PendingFunction {
    pub player: EntityId,
    pub position: Vec3,
    pub rotation: Vec2,
    pub function: Identifier,
}

/// The chat announcement of an advancement (`PlayerAdvancements.award`) that vanilla sends after the functions granted before
/// it ran: it waits in the same queue. `player_name` is the display name at the award, used when the player is gone by then.
pub struct PendingAnnouncement {
    pub player: EntityId,
    pub advancement: crate::advancements::AdvancementHolder,
    pub player_name: mc_text::Component,
}

/// An entry of the level's queue of rewards that wait for the awarded player to be back in the level.
pub enum PendingReward {
    Function(PendingFunction),
    Announcement(PendingAnnouncement),
}

/// The tick environment of the running tick with its lifetime erased (valid only inside `with_command_env`).
pub struct CommandEnvPtr(NonNull<dyn LevelTickEnv>);

impl ServerLevel {
    /// Makes `env` reachable through [`ServerLevel::perform_prefixed_command`] while `f` runs. `f` must not use `env` itself
    /// (the caller hands over its only borrow).
    pub(crate) fn with_command_env<R>(
        &mut self,
        env: &mut dyn LevelTickEnv,
        f: impl FnOnce(&mut ServerLevel) -> R,
    ) -> R {
        // SAFETY: only the lifetime is erased; the pointer is removed again before `env`'s borrow ends (also on unwind) and
        // `perform_prefixed_command` is the only user while `f` has the sole use of the borrow.
        let erased: &'static mut dyn LevelTickEnv = unsafe {
            std::mem::transmute::<&mut dyn LevelTickEnv, &'static mut dyn LevelTickEnv>(env)
        };
        let previous = self
            .tick_state
            .command_env
            .replace(CommandEnvPtr(NonNull::from(erased)));
        let result = panic::catch_unwind(AssertUnwindSafe(|| f(self)));
        self.tick_state.command_env = previous;
        match result {
            Ok(r) => r,
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    /// Whether a command can be run from this level right now (it is being ticked by a server).
    pub fn can_perform_commands(&self) -> bool {
        self.tick_state.command_env.is_some()
    }

    /// Queues a reward function of an awarded player (see [`PendingFunction`]).
    pub fn queue_function(&mut self, function: PendingFunction) {
        self.tick_state
            .pending_functions
            .push(PendingReward::Function(function));
    }

    /// Queues the chat announcement of an advancement behind the reward functions queued before it.
    pub fn queue_announcement(&mut self, announcement: PendingAnnouncement) {
        self.tick_state
            .pending_functions
            .push(PendingReward::Announcement(announcement));
    }

    pub fn has_pending_functions(&self) -> bool {
        !self.tick_state.pending_functions.is_empty()
    }

    /// The queued rewards, oldest first.
    pub fn take_pending_functions(&mut self) -> Vec<PendingReward> {
        std::mem::take(&mut self.tick_state.pending_functions)
    }

    /// `server.getCommands().performPrefixedCommand(source, command)` for a source located in this level. Panics when
    /// this level is not inside a server's tick (levels of tests without a server cannot run commands).
    pub fn perform_prefixed_command(&mut self, source: CommandSourceStack, command: &str) {
        let mut env = self
            .tick_state
            .command_env
            .take()
            .expect("commands can only run while a server ticks the level");
        // SAFETY: see `with_command_env`: the pointer was made from the live `&mut` of the tick environment and was taken out
        // above, so this is the only access to it until it is put back.
        unsafe { env.0.as_mut() }.perform_prefixed_command(self, source, command);
        self.tick_state.command_env = Some(env);
    }

    /// `Entity.teleportToPortalDestination` for an entity of this level that is out of its storage (its tick): the destination
    /// needs the server's other levels, which the tick environment reaches. Nothing happens without a server around the level.
    pub fn teleport_through_portal(&mut self, entity: &mut dyn crate::world::entity::Entity) {
        let Some(mut env) = self.tick_state.command_env.take() else {
            return;
        };
        // SAFETY: see `perform_prefixed_command`: the pointer is taken out of the level while it is used.
        unsafe { env.0.as_mut() }.teleport_through_portal(self, entity);
        self.tick_state.command_env = Some(env);
    }
}
