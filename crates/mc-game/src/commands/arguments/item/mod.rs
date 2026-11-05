//! `net.minecraft.commands.arguments.item`.
pub mod component_predicate_parser;
pub mod item_argument;
pub mod item_input;
pub mod item_parser;
mod item_predicate_argument;

pub use item_argument::ItemArgument;
pub use item_input::ItemInput;
pub use item_predicate_argument::{ItemPredicateArgument, ItemPredicateResult};
