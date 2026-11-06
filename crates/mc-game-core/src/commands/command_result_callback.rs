//! `net.minecraft.commands.CommandResultCallback`.

use std::sync::Arc;

#[derive(Clone, Default)]
pub enum CommandResultCallback {
    /// `CommandResultCallback.EMPTY`.
    #[default]
    Empty,
    Callback(Arc<dyn Fn(bool, i32) + Send + Sync>),
}

impl CommandResultCallback {
    pub const EMPTY: CommandResultCallback = CommandResultCallback::Empty;

    pub fn new(callback: impl Fn(bool, i32) + Send + Sync + 'static) -> Self {
        Self::Callback(Arc::new(callback))
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn on_result(&self, success: bool, result: i32) {
        if let Self::Callback(callback) = self {
            callback(success, result);
        }
    }

    pub fn on_success(&self, result: i32) {
        self.on_result(true, result);
    }

    pub fn on_failure(&self) {
        self.on_result(false, 0);
    }

    /// `CommandResultCallback.chain`: `first` runs, then `second`.
    pub fn chain(
        first: CommandResultCallback,
        second: CommandResultCallback,
    ) -> CommandResultCallback {
        match (first, second) {
            (Self::Empty, second) => second,
            (first, Self::Empty) => first,
            (first, second) => CommandResultCallback::new(move |success, result| {
                first.on_result(success, result);
                second.on_result(success, result);
            }),
        }
    }
}

impl PartialEq for CommandResultCallback {
    /// Java reference equality of the callback objects.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Callback(a), Self::Callback(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl std::fmt::Debug for CommandResultCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("<empty>"),
            Self::Callback(_) => f.write_str("<callback>"),
        }
    }
}
