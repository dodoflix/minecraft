//! `ReturnCommand`: `/return <value>`, `/return fail`, `/return run <command>`.
use mc_game_core::commands::execution::tasks::build_contexts::BuildContexts;
use mc_game_core::commands::execution::tasks::fallthrough_task;
use mc_game_core::commands::execution::{
    ChainModifiers, CustomCommandExecutor, CustomModifierExecutor, ExecutionControl, custom_command, custom_modifier,
};
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use mc_game_core::register_command;
use mc_brigadier::arguments::integer;
use mc_brigadier::{ArgumentBuilder, ContextChain};

struct ReturnFail;

impl CustomCommandExecutor for ReturnFail {
    fn run(&self, sender: &CommandSourceStack, _current_step: &ContextChain<CommandSourceStack>, _modifiers: ChainModifiers, output: &mut ExecutionControl<'_>) {
        sender.callback().on_failure();
        output.current_frame().return_failure();
        output.discard_current_frame();
    }
}

struct ReturnValue;

impl CustomCommandExecutor for ReturnValue {
    fn run(&self, sender: &CommandSourceStack, current_step: &ContextChain<CommandSourceStack>, _modifiers: ChainModifiers, output: &mut ExecutionControl<'_>) {
        let return_value = *current_step.get_top_context().get_argument::<i32>("value");
        sender.callback().on_success(return_value);
        output.current_frame().return_success(return_value);
        output.discard_current_frame();
    }
}

struct ReturnFromCommand;

impl CustomModifierExecutor for ReturnFromCommand {
    fn apply(
        &self,
        original_source: &CommandSourceStack,
        current_sources: Vec<CommandSourceStack>,
        current_step: &ContextChain<CommandSourceStack>,
        modifiers: ChainModifiers,
        output: &mut ExecutionControl<'_>,
    ) {
        if current_sources.is_empty() {
            if modifiers.is_return() {
                output.queue_next(fallthrough_task::instance());
            }
        } else {
            output.discard_current_frame();
            let next_state = current_step.next_stage().expect("`return run` is followed by a command stage");
            let command = next_state.get_top_context().get_input().to_owned();
            output.queue_next(BuildContexts::continuation(&command, next_state, modifiers.set_return(), original_source.clone(), current_sources));
        }
    }
}

register_command!("ReturnCommand", |ctx| {
    ctx.dispatcher.register(
        literal("return")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(argument("value", integer()).executes_command(custom_command(ReturnValue)))
            .then(literal("fail").executes_command(custom_command(ReturnFail)))
            .then(literal("run").forward(ctx.dispatcher.get_root().clone(), Some(custom_modifier(ReturnFromCommand)), false)),
    );
});
