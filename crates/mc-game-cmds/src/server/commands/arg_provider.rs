//! `net.minecraft.server.commands.ArgProvider`: one alternative of a "`<kind> <argument>`" command branch
//! (`entity <target>`, `block <pos>`, `storage <id>`), as the argument builder to attach and the accessor to resolve it.
use mc_game_core::commands::CommandSourceStack;
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, RequiredArgumentBuilder};
use std::sync::Arc;

type Access<T> = Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> Result<T, CommandSyntaxException> + Send + Sync>;

/// `ArgProvider<T>`.
pub struct ArgProvider<T> {
    key: &'static str,
    child: Arc<dyn Fn() -> RequiredArgumentBuilder<CommandSourceStack> + Send + Sync>,
    access: Access<T>,
}

impl<T> Clone for ArgProvider<T> {
    fn clone(&self) -> Self {
        Self { key: self.key, child: self.child.clone(), access: self.access.clone() }
    }
}

/// `ArgProvider.Factory<T>`: builds the provider for the argument name `arg` (`"target"`, `"source"`, ...).
pub type Factory<T> = fn(&'static str) -> ArgProvider<T>;

impl<T> ArgProvider<T> {
    /// `ArgProvider.create(key, child, access)`.
    pub fn create(
        key: &'static str,
        child: impl Fn() -> RequiredArgumentBuilder<CommandSourceStack> + Send + Sync + 'static,
        access: impl Fn(&CommandContext<CommandSourceStack>) -> Result<T, CommandSyntaxException> + Send + Sync + 'static,
    ) -> Self {
        Self { key, child: Arc::new(child), access: Arc::new(access) }
    }

    /// `access(context)`.
    pub fn access(&self, context: &CommandContext<CommandSourceStack>) -> Result<T, CommandSyntaxException> {
        (self.access)(context)
    }

    /// `wrap(parent, function)`: `parent.then(literal(key).then(function(child)))`.
    pub fn wrap<P: ArgumentBuilder<CommandSourceStack>, C: ArgumentBuilder<CommandSourceStack>>(
        &self,
        parent: P,
        function: impl FnOnce(RequiredArgumentBuilder<CommandSourceStack>) -> C,
    ) -> P {
        parent.then(mc_game_core::commands::literal(self.key).then(function((self.child)())))
    }

    /// `ArgProvider.buildList(argName, factories)`.
    pub fn build_list(arg_name: &'static str, factories: &[Factory<T>]) -> Vec<ArgProvider<T>> {
        factories.iter().map(|factory| factory(arg_name)).collect()
    }
}
