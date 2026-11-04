//! Port of `com.mojang.brigadier` 1.3.11 (the library vanilla Minecraft 26.3 ships).
//!
//! Differences from the Java API, all deliberate:
//! * All indices (cursor, ranges, error positions) are UTF-16 code units, exactly like Java.
//! * `CompletableFuture<Suggestions>` is just `Suggestions` (brigadier completes them synchronously).
//! * `ArgumentType` is generic over the source and returns a type-erased [`arguments::ParsedValue`].
//! * `CommandSyntaxException` is a `Result` error, never thrown; other Java runtime exceptions
//!   (`IllegalArgumentException` from `getArgument`, ...) are panics (see [`CommandContext::get_argument`]).
//! * The source type must be `Clone + 'static`.

pub mod arguments;
pub mod builder;
pub mod command;
pub mod context;
pub mod dispatcher;
pub mod exceptions;
pub mod java_fmt;
pub mod message;
pub mod string_reader;
pub mod suggestion;
pub mod tree;
pub mod utf16;

pub use arguments::{ArgumentType, ParsedValue};
pub use builder::{ArgumentBuilder, LiteralArgumentBuilder, NodeBuilder, RequiredArgumentBuilder, argument, literal};
pub use command::{
    AmbiguityConsumer, Command, Predicate, RedirectModifier, ResultConsumer, SINGLE_SUCCESS, SingleRedirectModifier,
};
pub use context::{
    CommandContext, CommandContextBuilder, ContextChain, IllegalArgumentException, ParsedArgument, ParsedCommandNode,
    Stage, StringRange, SuggestionContext,
};
pub use dispatcher::{ARGUMENT_SEPARATOR, ARGUMENT_SEPARATOR_CHAR, CommandDispatcher, ParseResults};
pub use exceptions::{
    BuiltInExceptionProvider, BuiltInExceptions, CommandExceptionType, CommandSyntaxException,
    Dynamic2CommandExceptionType, Dynamic3CommandExceptionType, Dynamic4CommandExceptionType,
    DynamicCommandExceptionType, DynamicNCommandExceptionType, SimpleCommandExceptionType,
};
pub use message::{LiteralMessage, Message, MessageRef};
pub use string_reader::{ImmutableStringReader, StringReader};
pub use suggestion::{Suggestion, SuggestionProvider, Suggestions, SuggestionsBuilder};
pub use tree::{CommandNode, NodeKind};
