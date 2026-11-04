use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// `com.mojang.brigadier.Message`. The server implements this for its chat `Component` and
/// recovers the concrete type with [`dyn Message::downcast_ref`].
pub trait Message: Any + Send + Sync + fmt::Debug {
    fn get_string(&self) -> String;
}

pub type MessageRef = Arc<dyn Message>;

impl dyn Message {
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref()
    }
}

/// `com.mojang.brigadier.LiteralMessage`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralMessage(pub String);

impl LiteralMessage {
    pub fn new(string: impl Into<String>) -> Self {
        Self(string.into())
    }
}

impl Message for LiteralMessage {
    fn get_string(&self) -> String {
        self.0.clone()
    }
}

impl fmt::Display for LiteralMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Shorthand for `Arc::new(LiteralMessage::new(s))`.
pub fn literal(string: impl Into<String>) -> MessageRef {
    Arc::new(LiteralMessage::new(string))
}
