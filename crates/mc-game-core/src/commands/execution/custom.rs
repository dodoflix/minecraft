//! `CustomCommandExecutor` / `CustomModifierExecutor`: nodes whose `Command`/`RedirectModifier` is not run by the
//! dispatcher but by the execution engine itself (`execute`, `return`, `function`, ...). Vanilla marks them by making
//! the node's command object also implement the interface (`instanceof` at run time). Rust closures cannot be
//! downcast, so [`custom_command`]/[`custom_modifier`] register the executor against the identity of the `Arc` they
//! return and the engine looks it up with [`lookup_custom_command`]/[`lookup_custom_modifier`].

use super::{ChainModifiers, ExecutionControl};
use crate::commands::CommandSourceStack;
use mc_brigadier::{Command, CommandSyntaxException, ContextChain, RedirectModifier};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

pub trait CustomCommandExecutor: Send + Sync {
    fn run(
        &self,
        sender: &CommandSourceStack,
        current_step: &ContextChain<CommandSourceStack>,
        modifiers: ChainModifiers,
        output: &mut ExecutionControl<'_>,
    );
}

pub trait CustomModifierExecutor: Send + Sync {
    fn apply(
        &self,
        original_source: &CommandSourceStack,
        current_sources: Vec<CommandSourceStack>,
        current_step: &ContextChain<CommandSourceStack>,
        modifiers: ChainModifiers,
        output: &mut ExecutionControl<'_>,
    );
}

/// `CustomCommandExecutor.WithErrorHandling.run`: a failure is reported to the sender and its callback.
pub fn run_with_error_handling(
    sender: &CommandSourceStack,
    modifiers: ChainModifiers,
    output: &mut ExecutionControl<'_>,
    run_guarded: impl FnOnce(&mut ExecutionControl<'_>) -> Result<(), CommandSyntaxException>,
) {
    if let Err(e) = run_guarded(output) {
        sender.handle_error(&e, modifiers.is_forked(), output.tracer());
        sender.callback().on_failure();
    }
}

type Registry<T> = LazyLock<Mutex<HashMap<usize, (Arc<dyn Fn() + Send + Sync>, Arc<T>)>>>;

static COMMANDS: Registry<dyn CustomCommandExecutor> = LazyLock::new(Default::default);
static MODIFIERS: Registry<dyn CustomModifierExecutor> = LazyLock::new(Default::default);

fn key<T: ?Sized>(arc: &Arc<T>) -> usize {
    Arc::as_ptr(arc) as *const () as usize
}

/// The `Command` to put on a node (`CommandAdapter`): running it through the dispatcher panics like vanilla's
/// `UnsupportedOperationException`; the execution engine runs `executor` instead.
pub fn custom_command(
    executor: impl CustomCommandExecutor + 'static,
) -> Command<CommandSourceStack> {
    let command: Command<CommandSourceStack> = Arc::new(|_| panic!("This function should not run"));
    // The registry keeps the command alive so its address is never reused by another allocation.
    let keep_alive: Arc<dyn Fn() + Send + Sync> = Arc::new({
        let command = command.clone();
        move || drop(command.clone())
    });
    COMMANDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key(&command), (keep_alive, Arc::new(executor)));
    command
}

/// `ModifierAdapter`: see [`custom_command`].
pub fn custom_modifier(
    executor: impl CustomModifierExecutor + 'static,
) -> RedirectModifier<CommandSourceStack> {
    let modifier: RedirectModifier<CommandSourceStack> =
        Arc::new(|_| panic!("This function should not run"));
    let keep_alive: Arc<dyn Fn() + Send + Sync> = Arc::new({
        let modifier = modifier.clone();
        move || drop(modifier.clone())
    });
    MODIFIERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key(&modifier), (keep_alive, Arc::new(executor)));
    modifier
}

pub fn lookup_custom_command(
    command: &Command<CommandSourceStack>,
) -> Option<Arc<dyn CustomCommandExecutor>> {
    COMMANDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key(command))
        .map(|(_, e)| e.clone())
}

pub fn lookup_custom_modifier(
    modifier: &RedirectModifier<CommandSourceStack>,
) -> Option<Arc<dyn CustomModifierExecutor>> {
    MODIFIERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key(modifier))
        .map(|(_, e)| e.clone())
}
