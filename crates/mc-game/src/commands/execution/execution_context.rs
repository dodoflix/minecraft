//! `net.minecraft.commands.execution.{ExecutionContext, ExecutionControl, CommandQueueEntry, EntryAction,
//! UnboundEntryAction}`. Vanilla's `ExecutionContext<T>` is generic over the source type; the only instantiation is
//! `CommandSourceStack`, so it is concrete here.

use super::frame::{Frame, FrameControl};
use super::tasks::build_contexts::BuildContexts;
use super::tasks::call_function::CallFunction;
use super::TraceCallbacks;
use crate::commands::functions::InstantiatedFunction;
use crate::commands::{CommandResultCallback, CommandSourceStack};
use mc_brigadier::ContextChain;
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_QUEUE_DEPTH: usize = 10_000_000;

/// `EntryAction`: runs once with the context and its frame.
pub type EntryAction = Box<dyn FnOnce(&mut ExecutionContext, &Frame)>;

/// `UnboundEntryAction`: an action that still needs its sender (function entries run once per sender).
pub trait UnboundEntryAction {
    fn execute(&self, sender: &CommandSourceStack, context: &mut ExecutionContext, frame: &Frame);
}

/// `UnboundEntryAction.bind(sender)`.
pub fn bind(action: Arc<dyn UnboundEntryAction>, sender: CommandSourceStack) -> EntryAction {
    Box::new(move |context, frame| action.execute(&sender, context, frame))
}

pub struct CommandQueueEntry {
    pub frame: Frame,
    pub action: EntryAction,
}

impl CommandQueueEntry {
    pub fn new(frame: Frame, action: EntryAction) -> CommandQueueEntry {
        CommandQueueEntry { frame, action }
    }

    pub fn execute(self, context: &mut ExecutionContext) {
        (self.action)(context, &self.frame);
    }
}

pub struct ExecutionContext {
    command_limit: i32,
    fork_limit: i32,
    tracer: Option<Box<dyn TraceCallbacks>>,
    command_quota: i32,
    queue_overflow: bool,
    command_queue: VecDeque<CommandQueueEntry>,
    new_top_commands: Vec<CommandQueueEntry>,
    current_frame_depth: i32,
}

impl ExecutionContext {
    pub fn new(command_limit: i32, fork_limit: i32) -> ExecutionContext {
        ExecutionContext {
            command_limit,
            fork_limit,
            tracer: None,
            command_quota: command_limit,
            queue_overflow: false,
            command_queue: VecDeque::new(),
            new_top_commands: Vec::new(),
            current_frame_depth: 0,
        }
    }

    fn create_top_frame(&self, frame_result: CommandResultCallback) -> Frame {
        if self.current_frame_depth == 0 {
            Frame::new(0, frame_result, FrameControl::ClearQueue)
        } else {
            let reentrant_frame_depth = self.current_frame_depth + 1;
            Frame::new(reentrant_frame_depth, frame_result, Self::frame_control_for_depth(reentrant_frame_depth))
        }
    }

    pub fn queue_initial_function_call(
        &mut self,
        function: Arc<InstantiatedFunction>,
        sender: CommandSourceStack,
        function_return: CommandResultCallback,
    ) {
        let frame = self.create_top_frame(function_return);
        let callback = sender.callback().clone();
        let action = bind(Arc::new(CallFunction::new(function, callback, false)), sender);
        self.queue_next(CommandQueueEntry::new(frame, action));
    }

    pub fn queue_initial_command_execution(
        &mut self,
        command: &str,
        execution_chain: ContextChain<CommandSourceStack>,
        sender: CommandSourceStack,
        command_return: CommandResultCallback,
    ) {
        let frame = self.create_top_frame(command_return);
        let action = BuildContexts::top_level(command, execution_chain, sender);
        self.queue_next(CommandQueueEntry::new(frame, action));
    }

    fn handle_queue_overflow(&mut self) {
        self.queue_overflow = true;
        self.new_top_commands.clear();
        self.command_queue.clear();
    }

    pub fn queue_next(&mut self, entry: CommandQueueEntry) {
        if self.new_top_commands.len() + self.command_queue.len() > MAX_QUEUE_DEPTH {
            self.handle_queue_overflow();
        }
        if !self.queue_overflow {
            self.new_top_commands.push(entry);
        }
    }

    pub fn discard_at_depth_or_higher(&mut self, depth_to_discard: i32) {
        while self.command_queue.front().is_some_and(|e| e.frame.depth() >= depth_to_discard) {
            self.command_queue.pop_front();
        }
    }

    pub fn frame_control_for_depth(depth_to_discard: i32) -> FrameControl {
        FrameControl::DiscardAtDepth(depth_to_discard)
    }

    pub(crate) fn apply_frame_control(&mut self, control: FrameControl) {
        match control {
            FrameControl::ClearQueue => self.command_queue.clear(),
            FrameControl::DiscardAtDepth(depth) => self.discard_at_depth_or_higher(depth),
        }
    }

    pub fn run_command_queue(&mut self) {
        self.push_new_commands();
        loop {
            if self.command_quota <= 0 {
                log::info!("Command execution stopped due to limit (executed {} commands)", self.command_limit);
                break;
            }
            let Some(command) = self.command_queue.pop_front() else {
                return;
            };
            self.current_frame_depth = command.frame.depth();
            command.execute(self);
            if self.queue_overflow {
                log::error!("Command execution stopped due to command queue overflow (max {})", MAX_QUEUE_DEPTH);
                break;
            }
            self.push_new_commands();
        }
        self.current_frame_depth = 0;
    }

    fn push_new_commands(&mut self) {
        for entry in self.new_top_commands.drain(..).rev() {
            self.command_queue.push_front(entry);
        }
    }

    pub fn set_tracer(&mut self, tracer: Option<Box<dyn TraceCallbacks>>) {
        self.tracer = tracer;
    }

    pub fn tracer(&mut self) -> Option<&mut dyn TraceCallbacks> {
        match &mut self.tracer {
            Some(tracer) => Some(tracer.as_mut()),
            None => None,
        }
    }

    pub fn fork_limit(&self) -> i32 {
        self.fork_limit
    }

    pub fn increment_cost(&mut self) {
        self.command_quota = self.command_quota.wrapping_sub(1);
    }

    /// `close()`.
    pub fn close(&mut self) {
        if let Some(tracer) = &mut self.tracer {
            tracer.close();
        }
    }
}

/// `ExecutionControl`: what custom executors get to queue follow-up work.
pub struct ExecutionControl<'a> {
    context: &'a mut ExecutionContext,
    frame: Frame,
}

impl<'a> ExecutionControl<'a> {
    pub fn create(context: &'a mut ExecutionContext, frame: Frame) -> ExecutionControl<'a> {
        ExecutionControl { context, frame }
    }

    pub fn queue_next(&mut self, action: EntryAction) {
        self.context.queue_next(CommandQueueEntry::new(self.frame.clone(), action));
    }

    pub fn set_tracer(&mut self, tracer: Option<Box<dyn TraceCallbacks>>) {
        self.context.set_tracer(tracer);
    }

    pub fn tracer(&mut self) -> Option<&mut dyn TraceCallbacks> {
        self.context.tracer()
    }

    pub fn current_frame(&self) -> &Frame {
        &self.frame
    }
}
