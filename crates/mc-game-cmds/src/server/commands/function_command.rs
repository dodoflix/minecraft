//! `FunctionCommand`: `/function <name> [<arguments> | with block|entity|storage ...]`, and the queueing of function calls
//! that `execute if function` and `/debug function` share the engine pieces of.
use crate::server::commands::arg_provider::ArgProvider;
use crate::server::commands::data::DataAccessor;
use crate::server::commands::data_commands::{Accessor, SOURCE_PROVIDERS, get_single_tag};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, ContextChain,
    Dynamic2CommandExceptionType, DynamicCommandExceptionType, Suggestions, SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_game_core::commands::arguments::function_argument::FunctionArgument;
use mc_game_core::commands::arguments::{CompoundTagArgument, NbtPathArgument};
use mc_game_core::commands::execution::tasks::call_function::CallFunction;
use mc_game_core::commands::execution::tasks::fallthrough_task;
use mc_game_core::commands::execution::{
    ChainModifiers, CustomCommandExecutor, ExecutionControl, bind, custom_command,
    run_with_error_handling,
};
use mc_game_core::commands::functions::CommandFunction;
use mc_game_core::commands::message::message;
use mc_game_core::commands::shared_suggestion_provider::{
    suggest_resource, suggest_resource_prefixed_into,
};
use mc_game_core::commands::{
    CommandResultCallback, CommandSourceStack, Commands, LEVEL_GAMEMASTERS, argument,
    has_permission, literal,
};
use mc_game_core::server::permissions::LevelBasedPermissionSet;
use mc_game_core::{register_command, translatable};
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use std::sync::{Arc, LazyLock, Mutex};

static ERROR_ARGUMENT_NOT_COMPOUND: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|type_name: String| {
            message(translatable!(
                "commands.function.error.argument_not_compound",
                type_name
            ))
        })
    });
static ERROR_NO_FUNCTIONS: LazyLock<DynamicCommandExceptionType<Component>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|name: Component| {
        message(translatable!(
            "commands.function.scheduled.no_functions",
            name
        ))
    })
});
/// `FunctionCommand.ERROR_FUNCTION_INSTANTATION_FAILURE`.
pub static ERROR_FUNCTION_INSTANTATION_FAILURE: LazyLock<
    Dynamic2CommandExceptionType<String, Component>,
> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|id: String, reason: Component| {
        message(translatable!(
            "commands.function.instantiationFailure",
            id,
            reason
        ))
    })
});

/// `Component.translationArg(id)`.
pub fn translation_arg(id: &Identifier) -> Component {
    Component::literal(id.to_string())
}

/// `FunctionCommand.SUGGEST_FUNCTION`: the function tags (`#`-prefixed), then the functions.
pub fn suggest_function(
    context: &CommandContext<CommandSourceStack>,
    mut builder: SuggestionsBuilder,
) -> Result<Suggestions, CommandSyntaxException> {
    let (tags, functions) = context.get_source().with_server(|server| {
        (
            server.functions().get_tag_names(),
            server.functions().get_function_names(),
        )
    });
    suggest_resource_prefixed_into(tags, &mut builder, "#");
    Ok(suggest_resource(functions, builder))
}

/// `FunctionCommand.FULL_CONTEXT_CALLBACKS.signalResult`.
fn signal_result(original_source: &CommandSourceStack, id: &Identifier, new_value: i32) {
    original_source.send_success(
        || translatable!("commands.function.result", translation_arg(id), new_value),
        true,
    );
}

/// `FunctionCommand.modifySenderForExecution`.
pub fn modify_sender_for_execution(sender: &CommandSourceStack) -> CommandSourceStack {
    sender
        .with_suppressed_output()
        .with_maximum_permission(&LevelBasedPermissionSet::GAMEMASTER.into())
}

/// `FunctionCommand.getArgumentTag`.
fn get_argument_tag(
    path: &mc_game_core::commands::arguments::NbtPath,
    accessor: &dyn DataAccessor,
    source: &CommandSourceStack,
) -> Result<CompoundTag, CommandSyntaxException> {
    match get_single_tag(path, accessor, source)? {
        Tag::Compound(compound) => Ok(compound),
        other => Err(ERROR_ARGUMENT_NOT_COMPOUND.create(Tag::type_name(other.id()))),
    }
}

type ArgumentsGetter = Box<
    dyn Fn(
            &CommandContext<CommandSourceStack>,
        ) -> Result<Option<CompoundTag>, CommandSyntaxException>
        + Send
        + Sync,
>;

/// `FunctionCommand.FunctionCustomExecutor`.
struct FunctionCustomExecutor {
    arguments: ArgumentsGetter,
}

impl CustomCommandExecutor for FunctionCustomExecutor {
    fn run(
        &self,
        sender: &CommandSourceStack,
        current_step: &ContextChain<CommandSourceStack>,
        modifiers: ChainModifiers,
        output: &mut ExecutionControl<'_>,
    ) {
        run_with_error_handling(sender, modifiers, output, |output| {
            let current_context = current_step.get_top_context().copy_for(sender.clone());
            let (id, functions) =
                FunctionArgument::get_function_collection(&current_context, "name")?;
            if functions.is_empty() {
                return Err(ERROR_NO_FUNCTIONS.create(translation_arg(&id)));
            }
            let arguments = (self.arguments)(&current_context)?;
            let common_function_context = modify_sender_for_execution(sender);
            if functions.len() == 1 {
                let function_id = functions[0].id().clone();
                sender.send_success(
                    || {
                        translatable!(
                            "commands.function.scheduled.single",
                            translation_arg(&function_id)
                        )
                    },
                    true,
                );
            } else {
                sender.send_success(
                    || {
                        translatable!(
                            "commands.function.scheduled.multiple",
                            mc_text::util::format_list(
                                functions.iter().map(|f| f.id().clone()),
                                &mc_text::util::default_separator(),
                                |id| translation_arg(&id)
                            )
                        )
                    },
                    true,
                );
            }
            queue_functions(
                &functions,
                arguments.as_ref(),
                sender,
                &common_function_context,
                output,
                modifiers,
            )
        });
    }
}

fn executor(
    arguments: impl Fn(
        &CommandContext<CommandSourceStack>,
    ) -> Result<Option<CompoundTag>, CommandSyntaxException>
    + Send
    + Sync
    + 'static,
) -> mc_brigadier::Command<CommandSourceStack> {
    custom_command(FunctionCustomExecutor {
        arguments: Box::new(arguments),
    })
}

register_command!("FunctionCommand", |ctx| {
    let mut sources = literal("with");
    for provider in SOURCE_PROVIDERS.iter() {
        let (whole, at_path): (ArgProvider<Accessor>, ArgProvider<Accessor>) =
            (provider.clone(), provider.clone());
        sources = provider.wrap(sources, move |p| {
            p.executes_command(executor(move |c| {
                Ok(Some(whole.access(c)?.get_data(c.get_source())?))
            }))
            .then(
                argument("path", NbtPathArgument::nbt_path()).executes_command(executor(
                    move |c| {
                        Ok(Some(get_argument_tag(
                            &NbtPathArgument::get_path(c, "path"),
                            &*at_path.access(c)?,
                            c.get_source(),
                        )?))
                    },
                )),
            )
        });
    }
    ctx.dispatcher.register(
        literal("function")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("name", FunctionArgument::functions())
                    .suggests(suggest_function)
                    .executes_command(executor(|_| Ok(None)))
                    .then(
                        argument("arguments", CompoundTagArgument::compound_tag())
                            .executes_command(executor(|c| {
                                Ok(Some(CompoundTagArgument::get_compound_tag(c, "arguments")))
                            })),
                    )
                    .then(sources),
            ),
    );
});

/// `FunctionCommand.queueFunctions`.
pub fn queue_functions(
    functions: &[Arc<dyn CommandFunction>],
    arguments: Option<&CompoundTag>,
    original_source: &CommandSourceStack,
    function_source: &CommandSourceStack,
    output: &mut ExecutionControl<'_>,
    modifiers: ChainModifiers,
) -> Result<(), CommandSyntaxException> {
    if modifiers.is_return() {
        queue_functions_as_return(
            functions,
            arguments,
            original_source,
            function_source,
            output,
        )
    } else {
        queue_functions_no_return(
            functions,
            arguments,
            original_source,
            function_source,
            output,
        )
    }
}

/// `instantiateAndQueueFunctions`.
fn instantiate_and_queue_functions(
    arguments: Option<&CompoundTag>,
    output: &mut ExecutionControl<'_>,
    commands: &Commands,
    no_callback_source: &CommandSourceStack,
    function: &Arc<dyn CommandFunction>,
    id: &Identifier,
    function_result_collector: CommandResultCallback,
    return_parent_frame: bool,
) -> Result<(), CommandSyntaxException> {
    let instantiated = function
        .instantiate(arguments, commands.get_dispatcher())
        .map_err(|e| {
            ERROR_FUNCTION_INSTANTATION_FAILURE.create(id.to_string(), *e.message_component)
        })?;
    output.queue_next(bind(
        Arc::new(CallFunction::new(
            instantiated,
            function_result_collector,
            return_parent_frame,
        )),
        no_callback_source.clone(),
    ));
    Ok(())
}

/// `decorateOutputIfNeeded`.
fn decorate_output_if_needed(
    original_source: &CommandSourceStack,
    id: &Identifier,
    callback: CommandResultCallback,
) -> CommandResultCallback {
    if original_source.is_silent() {
        callback
    } else {
        let (original_source, id) = (original_source.clone(), id.clone());
        CommandResultCallback::new(move |success, result| {
            signal_result(&original_source, &id, result);
            callback.on_result(success, result);
        })
    }
}

fn queue_functions_as_return(
    functions: &[Arc<dyn CommandFunction>],
    arguments: Option<&CompoundTag>,
    original_source: &CommandSourceStack,
    function_source: &CommandSourceStack,
    output: &mut ExecutionControl<'_>,
) -> Result<(), CommandSyntaxException> {
    let commands = original_source.with_server(|server| server.function_commands());
    let no_callback_source = function_source.clear_callbacks();
    let function_command_output_callback = CommandResultCallback::chain(
        original_source.callback().clone(),
        output.current_frame().return_value_consumer.clone(),
    );
    for function in functions {
        let id = function.id();
        let collector = decorate_output_if_needed(
            original_source,
            id,
            function_command_output_callback.clone(),
        );
        instantiate_and_queue_functions(
            arguments,
            output,
            &commands,
            &no_callback_source,
            function,
            id,
            collector,
            true,
        )?;
    }
    output.queue_next(fallthrough_task::instance());
    Ok(())
}

fn queue_functions_no_return(
    functions: &[Arc<dyn CommandFunction>],
    arguments: Option<&CompoundTag>,
    original_source: &CommandSourceStack,
    function_source: &CommandSourceStack,
    output: &mut ExecutionControl<'_>,
) -> Result<(), CommandSyntaxException> {
    let commands = original_source.with_server(|server| server.function_commands());
    let no_callback_source = function_source.clear_callbacks();
    let original_callback = original_source.callback().clone();
    if functions.is_empty() {
        return Ok(());
    }
    if functions.len() == 1 {
        let function = &functions[0];
        let id = function.id();
        let collector = decorate_output_if_needed(original_source, id, original_callback);
        instantiate_and_queue_functions(
            arguments,
            output,
            &commands,
            &no_callback_source,
            function,
            id,
            collector,
            false,
        )
    } else if original_callback.is_empty() {
        for function in functions {
            let id = function.id();
            let collector =
                decorate_output_if_needed(original_source, id, original_callback.clone());
            instantiate_and_queue_functions(
                arguments,
                output,
                &commands,
                &no_callback_source,
                function,
                id,
                collector,
                false,
            )?;
        }
        Ok(())
    } else {
        // `Accumulator`: (anyResult, sum)
        let accumulator = Arc::new(Mutex::new((false, 0i32)));
        let partial_result_callback = {
            let accumulator = accumulator.clone();
            CommandResultCallback::new(move |_success, result| {
                let mut a = accumulator.lock().unwrap_or_else(|e| e.into_inner());
                a.0 = true;
                a.1 = a.1.wrapping_add(result);
            })
        };
        for function in functions {
            let id = function.id();
            let collector =
                decorate_output_if_needed(original_source, id, partial_result_callback.clone());
            instantiate_and_queue_functions(
                arguments,
                output,
                &commands,
                &no_callback_source,
                function,
                id,
                collector,
                false,
            )?;
        }
        output.queue_next(Box::new(move |_context, _frame| {
            let (any_result, sum) = *accumulator.lock().unwrap_or_else(|e| e.into_inner());
            if any_result {
                original_callback.on_success(sum);
            }
        }));
        Ok(())
    }
}
