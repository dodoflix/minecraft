//! `tasks.ExecuteCommand`: runs the executable context of a command chain for one source.

use crate::commands::execution::{ChainModifiers, ExecutionContext, Frame, UnboundEntryAction};
use crate::commands::{CommandSourceStack, execution_command_source_result_consumer};
use mc_brigadier::{CommandContext, ContextChain};

pub struct ExecuteCommand {
    command_input: String,
    modifiers: ChainModifiers,
    execution_context: CommandContext<CommandSourceStack>,
}

impl ExecuteCommand {
    pub fn new(
        command_input: String,
        modifiers: ChainModifiers,
        execution_context: CommandContext<CommandSourceStack>,
    ) -> Self {
        Self {
            command_input,
            modifiers,
            execution_context,
        }
    }
}

impl UnboundEntryAction for ExecuteCommand {
    fn execute(&self, sender: &CommandSourceStack, context: &mut ExecutionContext, frame: &Frame) {
        context.increment_cost();
        match ContextChain::run_executable(
            &self.execution_context,
            sender.clone(),
            &execution_command_source_result_consumer(),
            self.modifiers.is_forked(),
        ) {
            Ok(result) => {
                if let Some(tracer) = context.tracer() {
                    tracer.on_return(frame.depth(), &self.command_input, result);
                }
            }
            Err(e) => sender.handle_error(&e, self.modifiers.is_forked(), context.tracer()),
        }
    }
}
