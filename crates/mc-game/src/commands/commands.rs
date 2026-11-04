//! `net.minecraft.commands.Commands`: the command dispatcher, its registration, execution entry points, failure
//! rendering and the command-tree packet.

use super::execution::ExecutionContext;
use super::host;
use super::message::from_message;
use super::{CommandBuildContext, CommandResultCallback, CommandSourceStack};
use crate::server::MinecraftServer;
use crate::server::permissions::{PermissionCheck, PermissionSet, permissions};
use crate::translatable;
use linkme::distributed_slice;
use mc_brigadier::utf16::{len16, substring, substring_from};
use mc_brigadier::{
    CommandDispatcher, CommandSyntaxException, ContextChain, LiteralArgumentBuilder, ParseResults, StringReader,
};
use mc_text::events::{ClickEvent, HoverEvent};
use mc_text::util::ellipsis;
use mc_text::{ChatFormatting, Component};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::null_mut;

pub const COMMAND_PREFIX: &str = "/";

/// `Commands.LEVEL_*` permission checks (`requires(Commands.hasPermission(..))`).
pub const LEVEL_ALL: PermissionCheck = PermissionCheck::AlwaysPass;
pub const LEVEL_MODERATORS: PermissionCheck = PermissionCheck::Require(permissions::COMMANDS_MODERATOR);
pub const LEVEL_GAMEMASTERS: PermissionCheck = PermissionCheck::Require(permissions::COMMANDS_GAMEMASTER);
pub const LEVEL_ADMINS: PermissionCheck = PermissionCheck::Require(permissions::COMMANDS_ADMIN);
pub const LEVEL_OWNERS: PermissionCheck = PermissionCheck::Require(permissions::COMMANDS_OWNER);

/// `Commands.hasPermission(check)`: a node requirement.
pub fn has_permission(check: PermissionCheck) -> impl Fn(&CommandSourceStack) -> bool + Send + Sync + 'static {
    move |source| check.check(source.permissions())
}

/// `Commands.literal`.
pub fn literal(name: impl Into<String>) -> LiteralArgumentBuilder<CommandSourceStack> {
    LiteralArgumentBuilder::literal(name)
}

/// `Commands.argument`.
pub fn argument(
    name: impl Into<String>,
    type_: impl mc_brigadier::ArgumentType<CommandSourceStack>,
) -> mc_brigadier::RequiredArgumentBuilder<CommandSourceStack> {
    mc_brigadier::argument(name, type_)
}

/// `Commands.CommandSelection`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandSelection {
    All,
    Dedicated,
    Integrated,
}

impl CommandSelection {
    pub fn include_integrated(self) -> bool {
        matches!(self, Self::All | Self::Integrated)
    }

    pub fn include_dedicated(self) -> bool {
        matches!(self, Self::All | Self::Dedicated)
    }
}

/// Everything a command's `register` function may need (`register(dispatcher, context)` plus the
/// `commandSelection != INTEGRATED` flags `SeedCommand`/`VersionCommand` take).
pub struct CommandRegistrationContext<'a> {
    pub dispatcher: &'a CommandDispatcher<CommandSourceStack>,
    pub build_context: &'a CommandBuildContext,
    pub selection: CommandSelection,
}

/// One command module (`XxxCommand.register`). `name` is the vanilla class's simple name (it fixes the registration
/// order, which is observable in the command tree packet: see [`REGISTRATION_ORDER`]).
pub struct CommandRegistration {
    pub name: &'static str,
    pub register: fn(&CommandRegistrationContext<'_>),
}

#[distributed_slice]
pub static COMMANDS: [CommandRegistration];

/// Registers a command module: `register_command!("StopCommand", |ctx| { ctx.dispatcher.register(...); })`.
#[macro_export]
macro_rules! register_command {
    ($name:literal, $register:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::commands::COMMANDS)]
            static COMMAND: $crate::commands::CommandRegistration =
                $crate::commands::CommandRegistration { name: $name, register: $register };
        };
    };
}

/// The order of the `register` calls in the `Commands` constructor. Node order in the tree packet follows it.
/// Groups: always, then dedicated-only, then integrated-only (vanilla's `if (includeDedicated)`/`includeIntegrated`).
pub const REGISTRATION_ORDER: &[&str] = &[
    "AdvancementCommands",
    "AttributeCommand",
    "ExecuteCommand",
    "BossBarCommands",
    "ClearInventoryCommands",
    "CloneCommands",
    "ComputeCommand",
    "DamageCommand",
    "DataCommands",
    "DataPackCommand",
    "DebugCommand",
    "DefaultGameModeCommands",
    "DialogCommand",
    "DifficultyCommand",
    "EffectCommands",
    "EmoteCommands",
    "EnchantCommand",
    "ExperienceCommand",
    "FillCommand",
    "FillBiomeCommand",
    "ForceLoadCommand",
    "FunctionCommand",
    "GameModeCommand",
    "GameRuleCommand",
    "GiveCommand",
    "HelpCommand",
    "ItemCommands",
    "KickCommand",
    "KillCommand",
    "ListPlayersCommand",
    "LocateCommand",
    "LootCommand",
    "MsgCommand",
    "SwingCommand",
    "ParticleCommand",
    "PlaceCommand",
    "PlaySoundCommand",
    "PostEffectCommand",
    "RandomCommand",
    "ReloadCommand",
    "RecipeCommand",
    "FetchProfileCommand",
    "ReturnCommand",
    "RideCommand",
    "RotateCommand",
    "SayCommand",
    "ScheduleCommand",
    "ScoreboardCommand",
    "SeedCommand",
    "VersionCommand",
    "SetBlockCommand",
    "SetSpawnCommand",
    "SetWorldSpawnCommand",
    "SpectateCommand",
    "SpreadPlayersCommand",
    "StopSoundCommand",
    "StopwatchCommand",
    "SummonCommand",
    "TagCommand",
    "TeamCommand",
    "TeamMsgCommand",
    "TeleportCommand",
    "TellRawCommand",
    "TestCommand",
    "TickCommand",
    "TimeCommand",
    "TitleCommand",
    "TriggerCommand",
    "WaypointCommand",
    "WeatherCommand",
    "WorldBorderCommand",
    "JfrCommand",
    // includeDedicated
    "BanIpCommands",
    "BanListCommands",
    "BanPlayerCommands",
    "DeOpCommands",
    "OpCommand",
    "PardonCommand",
    "PardonIpCommand",
    "PerfCommand",
    "SaveAllCommand",
    "SaveOffCommand",
    "SaveOnCommand",
    "SetPlayerIdleTimeoutCommand",
    "StopCommand",
    "TransferCommand",
    "WhitelistCommand",
    // includeIntegrated
    "PublishCommand",
    "UnpublishCommand",
];

const DEDICATED_ONLY: &[&str] = &[
    "BanIpCommands",
    "BanListCommands",
    "BanPlayerCommands",
    "DeOpCommands",
    "OpCommand",
    "PardonCommand",
    "PardonIpCommand",
    "PerfCommand",
    "SaveAllCommand",
    "SaveOffCommand",
    "SaveOnCommand",
    "SetPlayerIdleTimeoutCommand",
    "StopCommand",
    "TransferCommand",
    "WhitelistCommand",
];

const INTEGRATED_ONLY: &[&str] = &["PublishCommand", "UnpublishCommand"];

thread_local! {
    /// `Commands.CURRENT_EXECUTION_CONTEXT`.
    static CURRENT_EXECUTION_CONTEXT: Cell<*mut ExecutionContext> = const { Cell::new(null_mut()) };
}

pub struct Commands {
    dispatcher: CommandDispatcher<CommandSourceStack>,
}

impl Commands {
    /// `new Commands(selection, context)`: registers every linked command module in vanilla order.
    pub fn new(selection: CommandSelection, build_context: &CommandBuildContext) -> Commands {
        let mut dispatcher = CommandDispatcher::new();
        let ctx_dispatcher_ptr: *const CommandDispatcher<CommandSourceStack> = &dispatcher;
        // SAFETY: `register` only needs a shared reference for the duration of the calls below; `dispatcher` is not
        // moved or otherwise borrowed until they return.
        let context = CommandRegistrationContext { dispatcher: unsafe { &*ctx_dispatcher_ptr }, build_context, selection };
        let mut registrations: Vec<&CommandRegistration> = COMMANDS.iter().collect();
        for registration in &registrations {
            assert!(REGISTRATION_ORDER.contains(&registration.name), "command module {} is not in REGISTRATION_ORDER", registration.name);
        }
        registrations.sort_by_key(|r| REGISTRATION_ORDER.iter().position(|n| *n == r.name));
        for registration in registrations {
            let wanted = if DEDICATED_ONLY.contains(&registration.name) {
                selection.include_dedicated()
            } else if INTEGRATED_ONLY.contains(&registration.name) {
                selection.include_integrated()
            } else {
                true
            };
            if wanted {
                (registration.register)(&context);
            }
        }
        dispatcher.set_consumer(|context, success, result| context.get_source().callback().on_result(success, result));
        Commands { dispatcher }
    }

    /// A `Commands` over an already built dispatcher (tests, function compilation).
    pub fn from_dispatcher(dispatcher: CommandDispatcher<CommandSourceStack>) -> Commands {
        let mut dispatcher = dispatcher;
        dispatcher.set_consumer(|context, success, result| context.get_source().callback().on_result(success, result));
        Commands { dispatcher }
    }

    pub fn get_dispatcher(&self) -> &CommandDispatcher<CommandSourceStack> {
        &self.dispatcher
    }

    /// `mapSource`.
    pub fn map_source(
        parse: ParseResults<CommandSourceStack>,
        source_operator: impl FnOnce(&CommandSourceStack) -> CommandSourceStack,
    ) -> ParseResults<CommandSourceStack> {
        let mut context = parse.get_context().clone();
        let new_source = source_operator(context.get_source());
        context.with_source(new_source);
        ParseResults::new(context, parse.get_reader().clone(), parse.get_exceptions().to_vec())
    }

    pub fn trim_optional_prefix(command: &str) -> &str {
        command.strip_prefix('/').unwrap_or(command)
    }

    /// `performPrefixedCommand(sender, command)`.
    pub fn perform_prefixed_command(&self, server: &mut MinecraftServer, sender: CommandSourceStack, command: &str) {
        let command = Self::trim_optional_prefix(command);
        super::server_access::enter_server(server, || {
            let parse = self.dispatcher.parse(command, sender);
            self.perform_command_in_server(parse, command);
        });
    }

    /// `performCommand(parse, commandString)`.
    pub fn perform_command(&self, server: &mut MinecraftServer, parse: ParseResults<CommandSourceStack>, command_string: &str) {
        super::server_access::enter_server(server, || self.perform_command_in_server(parse, command_string));
    }

    fn perform_command_in_server(&self, parse: ParseResults<CommandSourceStack>, command_string: &str) {
        let sender = parse.get_context().get_source().clone();
        let command_chain = Self::finish_parsing(&parse, command_string, &sender);
        let Some(command_chain) = command_chain else {
            return;
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            Self::execute_command_in_context(&sender, |execution_context| {
                execution_context.queue_initial_command_execution(
                    command_string,
                    command_chain,
                    sender.clone(),
                    CommandResultCallback::EMPTY,
                );
            });
        }));
        if let Err(payload) = result {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "java.lang.RuntimeException".to_owned());
            log::error!("'/{}' threw an exception: {}", command_string, message);
            let hover = Component::literal(message);
            sender.send_failure(
                translatable!("command.failed")
                    .with_style_fn(|s| s.with_hover_event(Some(HoverEvent::show_text(hover.clone())))),
            );
        }
    }

    fn finish_parsing(
        command: &ParseResults<CommandSourceStack>,
        command_string: &str,
        sender: &CommandSourceStack,
    ) -> Option<ContextChain<CommandSourceStack>> {
        let result = (|| {
            Self::validate_parse_results(command)?;
            ContextChain::try_flatten(&command.get_context().build(command_string)).ok_or_else(|| {
                CommandSyntaxException::built_in_exceptions().dispatcher_unknown_command().create_with_context(command.get_reader())
            })
        })();
        match result {
            Ok(chain) => Some(chain),
            Err(e) => {
                sender.send_failure(from_message(e.get_raw_message().as_ref()));
                if let (Some(input), Some(cursor)) = (e.get_input(), e.get_cursor()) {
                    let cursor = cursor.min(len16(input));
                    let command_text = format!("/{command_string}");
                    let mut context = Component::empty()
                        .with_format(ChatFormatting::Gray)
                        .with_style_fn(|s| s.with_click_event(Some(ClickEvent::SuggestCommand(command_text.clone()))));
                    if cursor > 10 {
                        context = context.append(ellipsis());
                    }
                    context = context.append(Component::literal(substring(input, cursor.saturating_sub(10), cursor)));
                    if cursor < len16(input) {
                        let remaining = Component::literal(substring_from(input, cursor))
                            .with_formats(&[ChatFormatting::Red, ChatFormatting::Underline]);
                        context = context.append(remaining);
                    }
                    context = context
                        .append(translatable!("command.context.here").with_formats(&[ChatFormatting::Red, ChatFormatting::Italic]));
                    sender.send_failure(context);
                }
                None
            }
        }
    }

    /// `executeCommandInContext`: runs `config` (which queues work) in the current execution context, creating and
    /// running one when this is the outermost call.
    pub fn execute_command_in_context(context: &CommandSourceStack, config: impl FnOnce(&mut ExecutionContext)) {
        let current = CURRENT_EXECUTION_CONTEXT.with(|c| c.get());
        if current.is_null() {
            let (chain_limit, fork_limit) = context.with_server(host::command_limits);
            let mut execution_context = ExecutionContext::new(chain_limit.max(1), fork_limit);
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    CURRENT_EXECUTION_CONTEXT.with(|c| c.set(null_mut()));
                }
            }
            let _reset = Reset;
            CURRENT_EXECUTION_CONTEXT.with(|c| c.set(&mut execution_context as *mut ExecutionContext));
            config(&mut execution_context);
            execution_context.run_command_queue();
            execution_context.close();
        } else {
            // SAFETY: the pointer was set by the enclosing `execute_command_in_context` call that is still running on
            // this thread (it clears the cell when it ends) and is only dereferenced here, never while the outer
            // frame uses its `&mut` (the outer frame is inside `config`/`run_command_queue` calling into us).
            config(unsafe { &mut *current });
        }
    }

    /// `validateParseResults`.
    pub fn validate_parse_results<S: Clone + 'static>(command: &ParseResults<S>) -> Result<(), CommandSyntaxException> {
        match Self::get_parse_exception(command) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// `getParseException`.
    pub fn get_parse_exception<S: Clone + 'static>(parse: &ParseResults<S>) -> Option<CommandSyntaxException> {
        use mc_brigadier::ImmutableStringReader;
        if !parse.get_reader().can_read() {
            None
        } else if parse.get_exceptions().len() == 1 {
            Some(parse.get_exceptions()[0].1.clone())
        } else if parse.get_context().get_range().is_empty() {
            Some(CommandSyntaxException::built_in_exceptions().dispatcher_unknown_command().create_with_context(parse.get_reader()))
        } else {
            Some(CommandSyntaxException::built_in_exceptions().dispatcher_unknown_argument().create_with_context(parse.get_reader()))
        }
    }

    /// `Commands.createValidator(parser)`: a predicate over strings.
    pub fn create_validator(
        parser: impl Fn(&mut StringReader) -> Result<(), CommandSyntaxException> + Send + Sync + 'static,
    ) -> impl Fn(&str) -> bool + Send + Sync + 'static {
        move |value| parser(&mut StringReader::new(value)).is_ok()
    }

    /// `Commands.createCompilationContext`.
    pub fn create_compilation_context(compilation_permissions: PermissionSet) -> CommandSourceStack {
        CommandSourceStack::compilation_context(compilation_permissions)
    }
}

