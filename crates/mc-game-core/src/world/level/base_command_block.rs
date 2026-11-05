//! `net.minecraft.world.level.BaseCommandBlock`: the command, success count, last output and the execution of one
//! command for command blocks (and command minecarts).
//!
//! Vanilla mutates the object from inside the command (`successCount++` in the result callback, `lastOutput` in the
//! command source), while the object stays reachable through its block entity / entity. Here the two fields the command
//! touches live in a shared cell ([`Shared`]), so the command can run without the owner being borrowed: the owner calls
//! [`BaseCommandBlock::begin_perform`] (borrowing itself), runs the returned [`PendingCommand`] with the level, and
//! finishes with [`BaseCommandBlock::finish_perform`].

use crate::commands::server_access::{has_server, with_server};
use crate::commands::{CommandResultCallback, CommandSource, CommandSourceStack, null_source};
use crate::server::level::ServerLevel;
use crate::world::level::gamerules::GameRules;
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// `ComponentSerialization.CODEC` stored in `tag[key]` (`ValueOutput.storeNullable`).
pub(crate) fn put_component(tag: &mut CompoundTag, key: &str, component: &Component) {
    if let Ok(encoded) = mc_text::codec::encode_component::<Tag>(component) {
        tag.put(key, encoded);
    }
}

/// `BlockEntity.parseCustomNameSafe(input, key)`: the decoded component, `None` when absent or malformed.
pub(crate) fn parse_custom_name_safe(tag: &CompoundTag, key: &str) -> Option<Component> {
    mc_text::codec::decode_component::<Tag>(tag.get(key)?).ok()
}

/// The fields a running command changes.
struct Shared {
    success_count: AtomicI32,
    last_output: Mutex<Option<Component>>,
    /// Set by the command source when it stored output (`onUpdated(level)` is due).
    updated: AtomicBool,
}

pub struct BaseCommandBlock {
    last_execution: i64,
    update_last_execution: bool,
    shared: Arc<Shared>,
    track_output: bool,
    command: String,
    custom_name: Option<Component>,
}

impl Default for BaseCommandBlock {
    fn default() -> Self {
        Self {
            last_execution: -1,
            update_last_execution: true,
            shared: Arc::new(Shared { success_count: AtomicI32::new(0), last_output: Mutex::new(None), updated: AtomicBool::new(false) }),
            track_output: true,
            command: String::new(),
            custom_name: None,
        }
    }
}

/// `BaseCommandBlock.CloseableCommandBlockSource`.
struct CloseableCommandBlockSource {
    shared: Arc<Shared>,
    closed: AtomicBool,
    /// The rules at creation, used when the server is not reachable (it is borrowed by the running command handler).
    send_command_feedback: bool,
    command_block_output: bool,
}

fn live_rule(rule: crate::world::level::gamerules::GameRule<bool>, snapshot: bool) -> bool {
    if has_server() { with_server(|server| server.game_rules_ref().get(rule)) } else { snapshot }
}

impl CommandSource for CloseableCommandBlockSource {
    fn send_system_message(&self, message: &Component) {
        if !self.closed.load(Ordering::Relaxed) {
            let stamp = chrono::Local::now().format("%H:%M:%S");
            *self.shared.last_output.lock() = Some(Component::literal(format!("[{stamp}] ")).append(message.clone()));
            self.shared.updated.store(true, Ordering::Relaxed);
        }
    }

    fn accepts_success(&self) -> bool {
        !self.closed.load(Ordering::Relaxed) && live_rule(GameRules::SEND_COMMAND_FEEDBACK, self.send_command_feedback)
    }

    fn accepts_failure(&self) -> bool {
        !self.closed.load(Ordering::Relaxed)
    }

    fn should_inform_admins(&self) -> bool {
        !self.closed.load(Ordering::Relaxed) && live_rule(GameRules::COMMAND_BLOCK_OUTPUT, self.command_block_output)
    }
}

/// A command ready to run (the part of `performCommand` that needs the level and the server).
pub struct PendingCommand {
    stack: CommandSourceStack,
    command: String,
    source: Option<Arc<CloseableCommandBlockSource>>,
}

impl PendingCommand {
    /// `performPrefixedCommand` and the close of the command source.
    pub fn run(self, level: &mut ServerLevel) {
        level.perform_prefixed_command(self.stack, &self.command);
        if let Some(source) = self.source {
            source.closed.store(true, Ordering::Relaxed);
        }
    }
}

/// What [`BaseCommandBlock::begin_perform`] decided.
pub enum Begin {
    /// `performCommand` returned this value without running a command.
    Done(bool),
    /// Run the command, then call [`BaseCommandBlock::finish_perform`]; `performCommand` returns `true`.
    Run(PendingCommand),
}

impl BaseCommandBlock {
    /// `getSuccessCount()`.
    pub fn get_success_count(&self) -> i32 {
        self.shared.success_count.load(Ordering::Relaxed)
    }

    /// `setSuccessCount(successCount)`.
    pub fn set_success_count(&mut self, success_count: i32) {
        self.shared.success_count.store(success_count, Ordering::Relaxed);
    }

    /// `getLastOutput()` (`CommonComponents.EMPTY` when none).
    pub fn get_last_output(&self) -> Component {
        self.shared.last_output.lock().clone().unwrap_or_else(Component::empty)
    }

    /// `setLastOutput(lastOutput)`.
    pub fn set_last_output(&mut self, last_output: Option<Component>) {
        *self.shared.last_output.lock() = last_output;
    }

    /// `save(output)`.
    pub fn save(&self, tag: &mut CompoundTag) {
        tag.put_string("Command", self.command.clone());
        tag.put_int("SuccessCount", self.get_success_count());
        if let Some(name) = &self.custom_name {
            put_component(tag, "CustomName", name);
        }
        tag.put_boolean("TrackOutput", self.track_output);
        if self.track_output
            && let Some(last_output) = self.shared.last_output.lock().as_ref()
        {
            put_component(tag, "LastOutput", last_output);
        }
        tag.put_boolean("UpdateLastExecution", self.update_last_execution);
        if self.update_last_execution && self.last_execution != -1 {
            tag.put_long("LastExecution", self.last_execution);
        }
    }

    /// `load(input)`.
    pub fn load(&mut self, tag: &CompoundTag) {
        self.command = tag.get_string_or("Command", "").to_owned();
        self.set_success_count(tag.get_int_or("SuccessCount", 0));
        self.set_custom_name(parse_custom_name_safe(tag, "CustomName"));
        self.track_output = tag.get_boolean_or("TrackOutput", true);
        let last_output = if self.track_output { parse_custom_name_safe(tag, "LastOutput") } else { None };
        self.set_last_output(last_output);
        self.update_last_execution = tag.get_boolean_or("UpdateLastExecution", true);
        self.last_execution = if self.update_last_execution { tag.get_long_or("LastExecution", -1) } else { -1 };
    }

    /// `setCommand(command)` (the block entity's override adds `setChanged()`).
    pub fn set_command(&mut self, command: &str) {
        self.command = command.to_owned();
        self.set_success_count(0);
    }

    pub fn get_command(&self) -> &str {
        &self.command
    }

    /// `getName()`.
    pub fn get_name(&self) -> Component {
        self.custom_name.clone().unwrap_or_else(|| Component::literal("@"))
    }

    pub fn get_custom_name(&self) -> Option<&Component> {
        self.custom_name.as_ref()
    }

    pub fn set_custom_name(&mut self, name: Option<Component>) {
        self.custom_name = name;
    }

    pub fn set_track_output(&mut self, track_output: bool) {
        self.track_output = track_output;
    }

    pub fn is_track_output(&self) -> bool {
        self.track_output
    }

    /// Whether the command wrote output since the last call (`onUpdated(level)` is due).
    pub fn take_updated(&mut self) -> bool {
        self.shared.updated.swap(false, Ordering::Relaxed)
    }

    /// The first half of `performCommand(level)`: everything up to running the command.
    /// `create_stack(source)` is `createCommandSourceStack(level, source)` of the owner.
    pub fn begin_perform(&mut self, level: &ServerLevel, create_stack: impl FnOnce(Arc<dyn CommandSource>) -> CommandSourceStack) -> Begin {
        let game_time = level.game_time();
        if game_time == self.last_execution {
            return Begin::Done(false);
        }
        if self.command.eq_ignore_ascii_case("Searge") {
            *self.shared.last_output.lock() = Some(Component::literal("#itzlipofutzli"));
            self.set_success_count(1);
            return Begin::Done(true);
        }
        self.set_success_count(0);
        let rules = level.command_rules();
        if rules.command_blocks_work && !self.command.is_empty() {
            *self.shared.last_output.lock() = None;
            let source = self.track_output.then(|| {
                Arc::new(CloseableCommandBlockSource {
                    shared: self.shared.clone(),
                    closed: AtomicBool::new(false),
                    send_command_feedback: rules.send_command_feedback,
                    command_block_output: rules.command_block_output,
                })
            });
            let effective: Arc<dyn CommandSource> = match &source {
                Some(source) => source.clone(),
                None => null_source(),
            };
            let shared = self.shared.clone();
            let stack = create_stack(effective).with_callback(CommandResultCallback::new(move |success, _| {
                if success {
                    shared.success_count.fetch_add(1, Ordering::Relaxed);
                }
            }));
            return Begin::Run(PendingCommand { stack, command: self.command.clone(), source });
        }
        self.finish_perform(game_time);
        Begin::Done(true)
    }

    /// The tail of `performCommand`: `lastExecution`.
    pub fn finish_perform(&mut self, game_time: i64) {
        self.last_execution = if self.update_last_execution { game_time } else { -1 };
    }
}
