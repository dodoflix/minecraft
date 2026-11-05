//! `net.minecraft.commands.arguments.selector`: `@e[...]` selectors.
pub mod entity_selector;
pub mod entity_selector_options;
pub mod entity_selector_parser;
pub mod option_state;

pub use entity_selector::{EntityPredicate, EntityRef, EntitySelector, Order, SelectorContext};
pub use entity_selector_parser::EntitySelectorParser;
