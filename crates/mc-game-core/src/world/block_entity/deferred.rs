//! Level actions a block entity wants to run once it is back in its chunk.
//!
//! Vanilla runs them inline while the entity is in the chunk (`level.setBlockAndUpdate(ownPos, ..)` replaces the state
//! and must find the entity, entity moves collide with the entity's own block shape, comparators beside a container read
//! its items). Rust block entities are taken out of their chunk while their methods run (`ServerLevel::with_block_entity`),
//! so such actions are queued here and `with_block_entity` runs them right after putting the entity back.

use crate::server::level::ServerLevel;
use std::cell::{Cell, RefCell};

type Action = Box<dyn FnOnce(&mut ServerLevel)>;

thread_local! {
    /// `(depth of the with_block_entity call that queued it, action)`; depths never decrease along the queue.
    static QUEUE: RefCell<Vec<(u32, Action)>> = const { RefCell::new(Vec::new()) };
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// A `with_block_entity` call starts: actions queued from here run when it ends.
pub fn enter() {
    DEPTH.with(|d| d.set(d.get() + 1));
}

/// A `with_block_entity` call ended (the entity is back in its chunk): runs the actions queued inside it, including by
/// the calls nested in it that already ended.
pub fn leave(level: &mut ServerLevel) {
    let depth = DEPTH.with(|d| {
        d.set(d.get() - 1);
        d.get()
    });
    run_deeper_than(level, depth);
}

/// Queues `action` for the end of the running `with_block_entity`.
pub fn push(action: impl FnOnce(&mut ServerLevel) + 'static) {
    // outside any `with_block_entity` (a caller that took the entity by hand): the next call that ends runs it
    let depth = DEPTH.with(Cell::get).max(1);
    QUEUE.with(|q| q.borrow_mut().push((depth, Box::new(action))));
}

/// Runs `action` now when the entity is not out of its chunk (no `with_block_entity` is running, for example
/// `preRemoveSideEffects`), else queues it like [`push`].
pub fn run_or_push(level: &mut ServerLevel, action: impl FnOnce(&mut ServerLevel) + 'static) {
    if DEPTH.with(Cell::get) == 0 {
        action(level);
    } else {
        push(action);
    }
}

/// Runs the queued actions of calls deeper than `depth`, in order (actions queued meanwhile run too).
fn run_deeper_than(level: &mut ServerLevel, depth: u32) {
    loop {
        let actions = QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            let first = q.iter().position(|(d, _)| *d > depth).unwrap_or(q.len());
            q.split_off(first)
        });
        if actions.is_empty() {
            return;
        }
        for (_, action) in actions {
            action(level);
        }
    }
}
