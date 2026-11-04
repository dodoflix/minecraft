//! `com.mojang.brigadier.arguments`.
//!
//! Vanilla's `ArgumentType<T>` is typed by its result; here the trait is generic over the command
//! source `S` and returns a type-erased [`ParsedValue`], so the server can register its own argument
//! types (`impl<S: ...> ArgumentType<S> for EntityArgument`) and read results back with typed getters
//! ([`CommandContext::get_argument`]).

mod bool_argument;
mod number;
mod string_argument;

use std::any::Any;
use std::sync::Arc;

pub use bool_argument::BoolArgumentType;
pub use number::{
    DoubleArgumentType, FloatArgumentType, IntegerArgumentType, LongArgumentType, double_arg, double_arg_min,
    double_arg_range, float_arg, float_arg_min, float_arg_range, integer, integer_min, integer_range, long_arg,
    long_arg_min, long_arg_range,
};
pub use string_argument::{StringArgumentType, StringType, escape_if_required};

use crate::context::CommandContext;
use crate::exceptions::CommandSyntaxException;
use crate::string_reader::StringReader;
use crate::suggestion::{Suggestions, SuggestionsBuilder};

/// A parsed argument value, type-erased. Cheap to clone.
pub type ParsedValue = Arc<dyn Any + Send + Sync>;

/// Wraps a value as a [`ParsedValue`].
pub fn value<T: Any + Send + Sync>(v: T) -> ParsedValue {
    Arc::new(v)
}

pub trait ArgumentType<S>: Any + Send + Sync {
    /// `parse(StringReader)`.
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException>;

    /// `parse(StringReader, S)`; override when parsing depends on the source.
    fn parse_with_source(&self, reader: &mut StringReader, _source: &S) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse(reader)
    }

    fn list_suggestions(&self, _context: &CommandContext<S>, _builder: SuggestionsBuilder) -> Suggestions {
        Suggestions::empty()
    }

    fn get_examples(&self) -> Vec<String> {
        Vec::new()
    }

    /// Java `equals` (used for node equality); defaults to identity like `Object.equals`.
    fn equals(&self, other: &dyn ArgumentType<S>) -> bool {
        std::ptr::addr_eq(self as *const Self, other as *const dyn ArgumentType<S>)
    }

    /// Java `toString()`, shown in `<argument name:type>`.
    fn describe(&self) -> String {
        std::any::type_name::<Self>().to_owned()
    }
}

impl<S: 'static> dyn ArgumentType<S> {
    /// Downcasts to the concrete argument type (e.g. to look up its serializer).
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref()
    }
}
