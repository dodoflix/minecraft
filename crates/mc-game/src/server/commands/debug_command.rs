//! `DebugCommand`: `/debug start|stop|function <name>`.
use super::function_command::translation_arg;
use crate::commands::arguments::function_argument::FunctionArgument;
use crate::commands::execution::tasks::call_function::CallFunction;
use crate::commands::execution::{
    ChainModifiers, CustomCommandExecutor, ExecutionContext, ExecutionControl, Frame, TraceCallbacks, UnboundEntryAction, bind, custom_command,
    run_with_error_handling,
};
use crate::commands::message::simple_translatable;
use crate::commands::{CommandResultCallback, CommandSource, CommandSourceStack, LEVEL_ADMINS, argument, has_permission, literal};
use crate::server::commands::function_command::suggest_function;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::util::java_format::format_fixed;
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, ContextChain, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use mc_text::Component;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::{Arc, LazyLock, Mutex};

static ERROR_NOT_RUNNING: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.debug.notRunning"));
static ERROR_ALREADY_RUNNING: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.debug.alreadyRunning"));
static NO_RECURSIVE_TRACES: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.debug.function.noRecursion"));
static NO_RETURN_RUN: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.debug.function.noReturnRun"));

register_command!("DebugCommand", |ctx| {
    ctx.dispatcher.register(
        literal("debug")
            .requires(has_permission(LEVEL_ADMINS))
            .then(literal("start").executes(|c| start(c.get_source())))
            .then(literal("stop").executes(|c| stop(c.get_source())))
            .then(
                literal("function").requires(has_permission(LEVEL_ADMINS)).then(
                    argument("name", FunctionArgument::functions()).suggests(suggest_function).executes_command(custom_command(TraceCustomExecutor)),
                ),
            ),
    );
});

fn start(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|server| server.is_time_profiler_running()) {
        return Err(ERROR_ALREADY_RUNNING.create());
    }
    source.with_server(|server| server.start_time_profiler());
    source.send_success(|| translatable!("commands.debug.started"), true);
    Ok(0)
}

fn stop(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if !source.with_server(|server| server.is_time_profiler_running()) {
        return Err(ERROR_NOT_RUNNING.create());
    }
    let (nano_duration, tick_duration) = source.with_server(|server| server.stop_time_profiler());
    let seconds = nano_duration as f64 / 1_000_000_000.0;
    let tps = f64::from(tick_duration) / seconds;
    // `String.format("%.2f", NaN)` (an empty profile: 0 ticks over 0 seconds).
    let fixed = |v: f64| if v.is_nan() { "NaN".to_owned() } else { format_fixed(v, 2) };
    source.send_success(|| translatable!("commands.debug.stopped", fixed(seconds), tick_duration, fixed(tps)), true);
    Ok(tps as i32)
}

struct TracerState {
    output: BufWriter<File>,
    last_indent: i32,
    waiting_for_result: bool,
}

/// `DebugCommand.Tracer`: the trace file, as the functions' command source and as the engine's trace callbacks.
#[derive(Clone)]
struct Tracer(Arc<Mutex<TracerState>>);

impl TracerState {
    fn indent_and_save(&mut self, value: i32) {
        self.print_indent(value);
        self.last_indent = value;
    }

    fn print_indent(&mut self, value: i32) {
        for _ in 0..(value + 1).max(0) {
            let _ = self.output.write_all(b"    ");
        }
    }

    fn new_line(&mut self) {
        if self.waiting_for_result {
            let _ = self.output.write_all(b"\n");
            self.waiting_for_result = false;
        }
    }
}

impl Tracer {
    fn state(&self) -> std::sync::MutexGuard<'_, TracerState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl TraceCallbacks for Tracer {
    fn on_command(&mut self, depth: i32, command: &str) {
        let mut s = self.state();
        s.new_line();
        s.indent_and_save(depth);
        let _ = write!(s.output, "[C] {command}");
        s.waiting_for_result = true;
    }

    fn on_return(&mut self, depth: i32, command: &str, result: i32) {
        let mut s = self.state();
        if s.waiting_for_result {
            let _ = writeln!(s.output, " -> {result}");
            s.waiting_for_result = false;
        } else {
            s.indent_and_save(depth);
            let _ = writeln!(s.output, "[R = {result}] {command}");
        }
    }

    fn on_error(&mut self, message: &str) {
        let mut s = self.state();
        s.new_line();
        let indent = s.last_indent + 1;
        s.indent_and_save(indent);
        let _ = write!(s.output, "[E] {message}");
    }

    fn on_call(&mut self, depth: i32, function: &Identifier, size: usize) {
        let mut s = self.state();
        s.new_line();
        s.indent_and_save(depth);
        let _ = writeln!(s.output, "[F] {function} size={size}");
    }

    fn close(&mut self) {
        let _ = self.state().output.flush();
    }
}

impl CommandSource for Tracer {
    fn send_system_message(&self, message: &Component) {
        let mut s = self.state();
        s.new_line();
        let indent = s.last_indent + 1;
        s.print_indent(indent);
        let _ = writeln!(s.output, "[M] {}", message.get_string());
    }

    fn accepts_success(&self) -> bool {
        true
    }

    fn accepts_failure(&self) -> bool {
        true
    }

    fn should_inform_admins(&self) -> bool {
        false
    }

    fn always_accepts(&self) -> bool {
        true
    }
}

/// The anonymous `CallFunction` subclass that prints the function id before it runs.
struct PrintingCallFunction {
    inner: CallFunction,
    tracer: Tracer,
    id: Identifier,
}

impl UnboundEntryAction for PrintingCallFunction {
    fn execute(&self, sender: &CommandSourceStack, context: &mut ExecutionContext, frame: &Frame) {
        let _ = writeln!(self.tracer.state().output, "{}", self.id);
        self.inner.execute(sender, context, frame);
    }
}

struct TraceCustomExecutor;

impl CustomCommandExecutor for TraceCustomExecutor {
    fn run(&self, source: &CommandSourceStack, current_step: &ContextChain<CommandSourceStack>, modifiers: ChainModifiers, context: &mut ExecutionControl<'_>) {
        run_with_error_handling(source, modifiers, context, |context| {
            if modifiers.is_return() {
                return Err(NO_RETURN_RUN.create());
            }
            if context.tracer().is_some() {
                return Err(NO_RECURSIVE_TRACES.create());
            }
            let functions = FunctionArgument::get_functions(current_step.get_top_context(), "name")?;
            let output_name = format!("debug-trace-{}.txt", crate::crash_report::filename_formatted_date_time());
            let (commands, debug_dir) = source.with_server(|server| (server.function_commands(), server.get_file("debug")));
            // `CommandResponseTracker.track`: element count, sum of the sizes, the element when there is exactly one.
            let (mut element_count, mut total_value, mut only_element) = (0, 0i32, None);
            let traced = (|| -> std::io::Result<()> {
                std::fs::create_dir_all(&debug_dir)?;
                let tracer = Tracer(Arc::new(Mutex::new(TracerState {
                    output: BufWriter::new(File::create(debug_dir.join(&output_name))?),
                    last_indent: 0,
                    waiting_for_result: false,
                })));
                context.set_tracer(Some(Box::new(tracer.clone())));
                for function in &functions {
                    let function_source = source.with_source(Arc::new(tracer.clone())).with_maximum_permission(&LevelBasedPermissionSet::GAMEMASTER.into());
                    match function.instantiate(None, commands.get_dispatcher()) {
                        Ok(instantiated) => {
                            let size = instantiated.entries.len() as i32;
                            let action = PrintingCallFunction {
                                inner: CallFunction::new(instantiated, CommandResultCallback::EMPTY, false),
                                tracer: tracer.clone(),
                                id: function.id().clone(),
                            };
                            context.queue_next(bind(Arc::new(action), function_source));
                            total_value = total_value.wrapping_add(size);
                            element_count += 1;
                            only_element = if element_count == 1 { Some(function.id().clone()) } else { None };
                        }
                        Err(e) => source.send_failure(e.message_component),
                    }
                }
                Ok(())
            })();
            if let Err(e) = traced {
                log::warn!("Tracing failed\n{}", crate::server::packs::throwable("java.io.UncheckedIOException", &e.to_string()));
                source.send_failure(translatable!("commands.debug.function.traceFailed"));
            }
            let source = source.clone();
            context.queue_next(Box::new(move |_context, _frame| {
                source.send_success(
                    || match &only_element {
                        Some(id) => translatable!("commands.debug.function.success.single", total_value, translation_arg(id), output_name.clone()),
                        None => translatable!("commands.debug.function.success.multiple", total_value, element_count, output_name.clone()),
                    },
                    true,
                );
            }));
            Ok(())
        });
    }
}
