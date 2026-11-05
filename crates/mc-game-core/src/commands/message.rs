//! Bridge between brigadier `Message`s and chat `Component`s (`Component implements Message` in vanilla; the orphan
//! rule needs a newtype here).

use mc_brigadier::{LiteralMessage, Message, MessageRef};
use mc_text::Component;
use std::sync::Arc;

/// A `Message` that carries a chat component.
#[derive(Clone, Debug)]
pub struct ComponentMessage(pub Component);

impl Message for ComponentMessage {
    fn get_string(&self) -> String {
        self.0.get_string()
    }
}

/// `Component` used as a brigadier `Message`.
pub fn message(component: Component) -> MessageRef {
    Arc::new(ComponentMessage(component))
}

/// `ComponentUtils.fromMessage`.
pub fn from_message(message: &dyn Message) -> Component {
    match (message as &dyn std::any::Any).downcast_ref::<ComponentMessage>() {
        Some(component) => component.0.clone(),
        None => Component::literal(message.get_string()),
    }
}

/// `Component.translatable(key, args...)` / `translatableEscape`: every argument is converted with `Into<TranslatableArg>`.
#[macro_export]
macro_rules! translatable {
    ($key:expr) => {
        ::mc_text::Component::translatable($key, ::std::vec::Vec::new())
    };
    ($key:expr, $($arg:expr),+ $(,)?) => {
        ::mc_text::Component::translatable($key, ::std::vec![$(::mc_text::component::TranslatableArg::from($arg)),+])
    };
}

/// A `SimpleCommandExceptionType` whose message is `Component.translatable(key)`.
pub fn simple_translatable(key: &'static str) -> mc_brigadier::SimpleCommandExceptionType {
    mc_brigadier::SimpleCommandExceptionType::new(message(Component::translatable(key, Vec::new())))
}

/// `new DynamicCommandExceptionType(a -> Component.translatableEscape(key, a))` for string-like arguments.
pub fn dynamic_translatable(key: &'static str) -> mc_brigadier::DynamicCommandExceptionType<String> {
    mc_brigadier::DynamicCommandExceptionType::new(move |a: String| message(Component::translatable(key, vec![a.into()])))
}

/// `LiteralMessage`.
pub fn literal_message(text: impl Into<String>) -> MessageRef {
    Arc::new(LiteralMessage::new(text))
}
