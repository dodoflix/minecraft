//! Component value families. Each module exports the value types, the typed `ComponentKey` constants (named like
//! vanilla's `DataComponents` constants) and `defs()`, which registers the component definitions.

use super::component::ComponentDef;
use super::item_stack::ItemStack;

pub mod basic;
pub mod combat;
pub mod containers;
pub mod decor;
pub mod effects;
pub mod entity;
pub mod predicates;
pub mod profile;

pub use basic::*;
pub use combat::*;
pub use containers::*;
pub use decor::*;
pub use effects::*;
pub use entity::*;
pub use predicates::*;
pub use profile::*;

pub(crate) fn collect_defs(out: &mut Vec<ComponentDef>) {
    basic::defs(out);
    combat::defs(out);
    containers::defs(out);
    decor::defs(out);
    effects::defs(out);
    entity::defs(out);
    predicates::defs(out);
    profile::defs(out);
}

/// `ItemStack.validateComponents`.
pub(crate) fn validate_components(stack: &ItemStack) -> Result<(), String> {
    use super::map::DataComponentGetter;
    let Some(components) = stack.components() else {
        return Ok(());
    };
    if components.has(MAX_DAMAGE.id()) && *components.get_or_default(&MAX_STACK_SIZE, &1) > 1 {
        return Err("Item cannot be both damageable and stackable".into());
    }
    if let Some(container) = components.get(&CONTAINER) {
        containers::validate_contained_item_sizes(container.non_empty_items())?;
    }
    if let Some(bundle) = components.get(&BUNDLE_CONTENTS) {
        containers::validate_contained_item_sizes(bundle.items())?;
        bundle.weight()?;
    }
    if let Some(charged) = components.get(&CHARGED_PROJECTILES) {
        containers::validate_contained_item_sizes(&charged.items)?;
    }
    Ok(())
}
