//! `Command`, `RedirectModifier`, `SingleRedirectModifier`, `ResultConsumer`, `AmbiguityConsumer`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::context::CommandContext;
use crate::exceptions::CommandSyntaxException;
use crate::tree::CommandNode;

/// `Command.SINGLE_SUCCESS`.
pub const SINGLE_SUCCESS: i32 = 1;

/// `Command<S>`: returns the command result.
pub type Command<S> =
    Arc<dyn Fn(&CommandContext<S>) -> Result<i32, CommandSyntaxException> + Send + Sync>;

/// `RedirectModifier<S>`: maps the context to the sources the redirect target runs with.
pub type RedirectModifier<S> =
    Arc<dyn Fn(&CommandContext<S>) -> Result<Vec<S>, CommandSyntaxException> + Send + Sync>;

/// `SingleRedirectModifier<S>`.
pub type SingleRedirectModifier<S> =
    Arc<dyn Fn(&CommandContext<S>) -> Result<S, CommandSyntaxException> + Send + Sync>;

/// Node requirement (`Predicate<S>`).
pub type Predicate<S> = Arc<dyn Fn(&S) -> bool + Send + Sync>;

/// `ResultConsumer<S>`: `(context, success, result)`.
pub type ResultConsumer<S> = Arc<dyn Fn(&CommandContext<S>, bool, i32) + Send + Sync>;

/// `AmbiguityConsumer<S>`: `(parent, child, sibling, inputs both accept)`.
pub type AmbiguityConsumer<'a, S> = &'a mut dyn FnMut(
    &Arc<CommandNode<S>>,
    &Arc<CommandNode<S>>,
    &Arc<CommandNode<S>>,
    &HashSet<String>,
);
