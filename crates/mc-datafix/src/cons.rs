//! Weak hash-consing of immutable nodes: vanilla builds fresh `Type`s and optics for every `apply(family).apply(index)`
//! and the garbage collector drops what no rewrite result references. Here equal constructions share one allocation,
//! held weakly so that nodes nobody uses are freed. Sharing is invisible to the structural `equals` of types and optics.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, Weak};

pub(crate) struct WeakCons<T> {
    table: Mutex<Table<T>>,
}

struct Table<T> {
    nodes: HashMap<u64, Weak<T>>,
    /// Table size that triggers the next sweep of dead entries.
    sweep_at: usize,
}

const MIN_SWEEP: usize = 1 << 16;

/// The hash of a node's identifying parts (children by address, names, indices).
pub(crate) fn hash_parts(parts: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

impl<T> WeakCons<T> {
    pub(crate) fn new() -> WeakCons<T> {
        WeakCons {
            table: Mutex::new(Table {
                nodes: HashMap::new(),
                sweep_at: MIN_SWEEP,
            }),
        }
    }

    /// The live node of `hash` when `same` accepts it, else `make()`; a new node takes the slot of `hash` unless a live
    /// node of another shape holds it (a 64 bit collision: the node is then simply not shared).
    pub(crate) fn get_or_make(
        &self,
        hash: u64,
        same: impl Fn(&T) -> bool,
        make: impl FnOnce() -> Arc<T>,
    ) -> Arc<T> {
        let mut table = self.table.lock().unwrap();
        if let Some(node) = table.nodes.get(&hash).and_then(Weak::upgrade) {
            if same(&node) {
                return node;
            }
            return make();
        }
        let node = make();
        if table.nodes.len() >= table.sweep_at {
            table.nodes.retain(|_, w| w.strong_count() > 0);
            table.sweep_at = (table.nodes.len() * 2).max(MIN_SWEEP);
        }
        table.nodes.insert(hash, Arc::downgrade(&node));
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(cons: &WeakCons<u32>, hash: u64, value: u32) -> Arc<u32> {
        cons.get_or_make(hash, |v| *v == value, || Arc::new(value))
    }

    #[test]
    fn equal_nodes_are_shared_while_alive_and_rebuilt_after() {
        let cons = WeakCons::new();
        let a = get(&cons, 1, 7);
        assert!(Arc::ptr_eq(&a, &get(&cons, 1, 7)));
        drop(a);
        assert_eq!(*get(&cons, 1, 7), 7);
    }

    #[test]
    fn a_collision_with_another_shape_is_not_shared() {
        let cons = WeakCons::new();
        let a = get(&cons, 1, 7);
        let b = get(&cons, 1, 8);
        assert_eq!((*a, *b), (7, 8));
        assert!(
            Arc::ptr_eq(&a, &get(&cons, 1, 7)),
            "the slot keeps the first node"
        );
    }
}
