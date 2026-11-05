//! `CallFunction`.

use super::continuation_task;
use crate::commands::execution::{CommandQueueEntry, ExecutionContext, Frame, UnboundEntryAction, bind};
use crate::commands::functions::InstantiatedFunction;
use crate::commands::{CommandResultCallback, CommandSourceStack};
use std::sync::Arc;

pub struct CallFunction {
    function: Arc<InstantiatedFunction>,
    result_callback: CommandResultCallback,
    return_parent_frame: bool,
}

impl CallFunction {
    pub fn new(function: Arc<InstantiatedFunction>, result_callback: CommandResultCallback, return_parent_frame: bool) -> Self {
        Self { function, result_callback, return_parent_frame }
    }
}

impl UnboundEntryAction for CallFunction {
    fn execute(&self, sender: &CommandSourceStack, context: &mut ExecutionContext, frame: &Frame) {
        context.increment_cost();
        let contents = self.function.entries.clone();
        if let Some(tracer) = context.tracer() {
            tracer.on_call(frame.depth(), &self.function.id, self.function.entries.len());
        }
        let new_depth = frame.depth() + 1;
        let frame_control =
            if self.return_parent_frame { frame.frame_control() } else { ExecutionContext::frame_control_for_depth(new_depth) };
        let new_frame = Frame::new(new_depth, self.result_callback.clone(), frame_control);
        let sender = sender.clone();
        continuation_task::schedule(context, &new_frame, contents, move |frame, entry| {
            CommandQueueEntry::new(frame.clone(), bind(entry.clone(), sender.clone()))
        });
    }
}
