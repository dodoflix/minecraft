//! `Holder<T>`: a reference to a registry entry that is bound after the whole registry has been decoded (entries may
//! reference each other and themselves through holders, like vanilla's `Holder.Reference`), or a direct value.

use mc_core::ident::Identifier;
use std::fmt;
use std::sync::{Arc, OnceLock};

pub struct Holder<T: ?Sized> {
    id: Option<Identifier>,
    cell: Arc<OnceLock<Arc<T>>>,
}

impl<T: ?Sized> Clone for Holder<T> {
    fn clone(&self) -> Self {
        Holder { id: self.id.clone(), cell: self.cell.clone() }
    }
}

impl<T: ?Sized> Holder<T> {
    /// `Holder.direct(value)`.
    pub fn direct(value: Arc<T>) -> Holder<T> {
        let cell = OnceLock::new();
        let _ = cell.set(value);
        Holder { id: None, cell: Arc::new(cell) }
    }

    /// A reference holder whose cell is shared with the registry entry `id` (bound by [`Holder::bind`]).
    pub fn reference(id: Identifier, cell: Arc<OnceLock<Arc<T>>>) -> Holder<T> {
        Holder { id: Some(id), cell }
    }

    pub fn bind(&self, value: Arc<T>) {
        let _ = self.cell.set(value);
    }

    pub fn is_bound(&self) -> bool {
        self.cell.get().is_some()
    }

    /// `unwrapKey()`: the registry id of a reference holder.
    pub fn id(&self) -> Option<&Identifier> {
        self.id.as_ref()
    }

    /// `value()`; panics for an unbound holder (a registry that was not fully loaded).
    pub fn value(&self) -> &Arc<T> {
        self.cell.get().unwrap_or_else(|| panic!("Trying to access unbound value '{}' from registry", self.id.as_ref().map_or_else(|| "<direct>".to_owned(), ToString::to_string)))
    }

    /// Whether two holders point at the same registry entry/value (`Holder.equals`).
    pub fn same(&self, other: &Holder<T>) -> bool {
        Arc::ptr_eq(&self.cell, &other.cell)
    }
}

impl<T: ?Sized> fmt::Debug for Holder<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.id {
            Some(id) => write!(f, "Reference{{{id}}}"),
            None => f.write_str("Direct{..}"),
        }
    }
}
