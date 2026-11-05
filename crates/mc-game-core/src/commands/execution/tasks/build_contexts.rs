//! `BuildContexts`: walks a flattened command chain, applying redirect/fork modifiers, then queues the execution of
//! the final command for every resulting source.

use super::continuation_task;
use super::execute_command::ExecuteCommand;
use super::fallthrough_task;
use crate::commands::execution::{
    ChainModifiers, CommandQueueEntry, EntryAction, ExecutionContext, ExecutionControl, Frame, UnboundEntryAction, bind,
    lookup_custom_command, lookup_custom_modifier,
};
use crate::commands::{CommandResultCallback, CommandSourceStack};
use crate::commands::message::message;
use mc_brigadier::{ContextChain, DynamicCommandExceptionType, Stage};
use std::sync::{Arc, LazyLock};

/// `BuildContexts.ERROR_FORK_LIMIT_REACHED`.
pub static ERROR_FORK_LIMIT_REACHED: LazyLock<DynamicCommandExceptionType<i32>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|limit: i32| message(crate::translatable!("command.forkLimit", limit))));

pub struct BuildContexts;

impl BuildContexts {
    fn trace_command_start(context: &mut ExecutionContext, frame: &Frame, command_input: &str) {
        if let Some(tracer) = context.tracer() {
            tracer.on_command(frame.depth(), command_input);
        }
    }

    /// `BuildContexts.TopLevel`.
    pub fn top_level(command_input: &str, command: ContextChain<CommandSourceStack>, source: CommandSourceStack) -> EntryAction {
        let command_input = command_input.to_owned();
        Box::new(move |context, frame| {
            Self::trace_command_start(context, frame, &command_input);
            Self::execute(&command_input, &command, &source, vec![source.clone()], context, frame, ChainModifiers::DEFAULT);
        })
    }

    /// `BuildContexts.Continuation` (what custom executors queue to resume a chain).
    pub fn continuation(
        command_input: &str,
        command: ContextChain<CommandSourceStack>,
        modifiers: ChainModifiers,
        original_source: CommandSourceStack,
        sources: Vec<CommandSourceStack>,
    ) -> EntryAction {
        let command_input = command_input.to_owned();
        Box::new(move |context, frame| {
            Self::execute(&command_input, &command, &original_source, sources, context, frame, modifiers);
        })
    }

    /// `BuildContexts.Unbound` (a command line stored in a function).
    pub fn unbound(command_input: &str, command: ContextChain<CommandSourceStack>) -> Arc<dyn UnboundEntryAction> {
        Arc::new(Unbound { command_input: command_input.to_owned(), command })
    }

    fn execute(
        command_input: &str,
        command: &ContextChain<CommandSourceStack>,
        original_source: &CommandSourceStack,
        initial_sources: Vec<CommandSourceStack>,
        context: &mut ExecutionContext,
        frame: &Frame,
        initial_modifiers: ChainModifiers,
    ) {
        let mut current_stage = command.clone();
        let mut modifiers = initial_modifiers;
        let mut current_sources = initial_sources;
        if current_stage.get_stage() != Stage::Execute {
            let fork_limit = context.fork_limit();
            while current_stage.get_stage() != Stage::Execute {
                let context_to_run = current_stage.get_top_context();
                if context_to_run.is_forked() {
                    modifiers = modifiers.set_forked();
                }
                let modifier = context_to_run.get_redirect_modifier();
                if let Some(custom) = modifier.and_then(lookup_custom_modifier) {
                    custom.apply(
                        original_source,
                        current_sources,
                        &current_stage,
                        modifiers,
                        &mut ExecutionControl::create(context, frame.clone()),
                    );
                    return;
                }
                if modifier.is_some() {
                    context.increment_cost();
                    let forked_mode = modifiers.is_forked();
                    let noop_consumer: mc_brigadier::ResultConsumer<CommandSourceStack> = Arc::new(|_, _, _| {});
                    let mut next_sources: Vec<CommandSourceStack> = Vec::new();
                    for source in &current_sources {
                        match ContextChain::run_modifier(context_to_run, source.clone(), &noop_consumer, forked_mode) {
                            Ok(new_sources) => {
                                if (next_sources.len() + new_sources.len()) as i64 >= i64::from(fork_limit) {
                                    original_source.handle_error(&ERROR_FORK_LIMIT_REACHED.create(fork_limit), forked_mode, context.tracer());
                                    return;
                                }
                                next_sources.extend(new_sources);
                            }
                            Err(e) => {
                                source.handle_error(&e, forked_mode, context.tracer());
                                if !forked_mode {
                                    return;
                                }
                            }
                        }
                    }
                    current_sources = next_sources;
                }
                current_stage = current_stage.next_stage().expect("a modify stage has a next stage");
            }
        }
        if current_sources.is_empty() {
            if modifiers.is_return() {
                context.queue_next(CommandQueueEntry::new(frame.clone(), fallthrough_task::instance()));
            }
            return;
        }
        let execute_context = current_stage.get_top_context();
        if let Some(custom) = execute_context.get_command().and_then(lookup_custom_command) {
            let mut control = ExecutionControl::create(context, frame.clone());
            for execution_source in &current_sources {
                custom.run(execution_source, &current_stage, modifiers, &mut control);
            }
        } else {
            if modifiers.is_return() {
                let returning_source = &current_sources[0];
                let returning_source = returning_source.with_callback(CommandResultCallback::chain(
                    returning_source.callback().clone(),
                    frame.return_value_consumer.clone(),
                ));
                current_sources = vec![returning_source];
            }
            let action: Arc<dyn UnboundEntryAction> =
                Arc::new(ExecuteCommand::new(command_input.to_owned(), modifiers, execute_context.clone()));
            continuation_task::schedule(context, frame, current_sources, move |frame, source| {
                CommandQueueEntry::new(frame.clone(), bind(action.clone(), source.clone()))
            });
        }
    }
}

struct Unbound {
    command_input: String,
    command: ContextChain<CommandSourceStack>,
}

impl UnboundEntryAction for Unbound {
    fn execute(&self, sender: &CommandSourceStack, context: &mut ExecutionContext, frame: &Frame) {
        BuildContexts::trace_command_start(context, frame, &self.command_input);
        BuildContexts::execute(&self.command_input, &self.command, sender, vec![sender.clone()], context, frame, ChainModifiers::DEFAULT);
    }
}

