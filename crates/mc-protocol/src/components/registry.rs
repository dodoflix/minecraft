//! The registry of component definitions, indexed by `mc_data::DataComponentType`.

use super::component::ComponentDef;
use mc_data::DataComponentType;
use std::sync::LazyLock;

static DEFS: LazyLock<Vec<Option<ComponentDef>>> = LazyLock::new(|| {
    let mut all: Vec<ComponentDef> = Vec::new();
    super::values::collect_defs(&mut all);
    let mut table: Vec<Option<ComponentDef>> = (0..DataComponentType::registry().len())
        .map(|_| None)
        .collect();
    for d in all {
        let i = d.id.id() as usize;
        assert!(table[i].is_none(), "component {} defined twice", d.name());
        table[i] = Some(d);
    }
    table
});

/// The definition of `id`; panics if the component's family has not been implemented (see the coverage test).
pub fn def(id: DataComponentType) -> &'static ComponentDef {
    DEFS[id.id() as usize]
        .as_ref()
        .unwrap_or_else(|| panic!("data component {} is not implemented", id.name()))
}

/// Like [`def`] but `None` for components without a definition.
pub fn try_def(id: DataComponentType) -> Option<&'static ComponentDef> {
    DEFS[id.id() as usize].as_ref()
}

/// Ids of the components that have a definition.
pub fn defined() -> impl Iterator<Item = DataComponentType> {
    DataComponentType::iter().filter(|t| try_def(*t).is_some())
}
