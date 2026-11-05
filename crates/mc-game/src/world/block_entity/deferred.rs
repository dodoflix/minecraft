//! Level actions a block entity wants to run once it is back in its chunk.
//!
//! Vanilla runs them inline while the entity is in the chunk (`level.setBlockAndUpdate(ownPos, ..)` replaces the state
//! and must find the entity, entity moves collide with the entity's own block shape). Rust block entities are taken out
//! of their chunk while their methods run (`ServerLevel::with_block_entity`), so such actions are queued here and
//! `with_block_entity` runs them right after putting the entity back.

use crate::server::level::ServerLevel;
use std::cell::{Cell, RefCell};

type Action = Box<dyn FnOnce(&mut ServerLevel)>;

thread_local! {
    static QUEUE: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// A `with_block_entity` call starts: actions queued from here run when the outermost call ends.
pub fn enter() {
    DEPTH.with(|d| d.set(d.get() + 1));
}

/// A `with_block_entity` call ended (the entity is back in its chunk): the outermost call runs the queue.
pub fn leave(level: &mut ServerLevel) {
    let outermost = DEPTH.with(|d| {
        d.set(d.get() - 1);
        d.get() == 0
    });
    if outermost {
        run_all(level);
    }
}

/// Queues `action` for the end of the running `with_block_entity`.
pub fn push(action: impl FnOnce(&mut ServerLevel) + 'static) {
    QUEUE.with(|q| q.borrow_mut().push(Box::new(action)));
}

/// Runs the queued actions in order (actions queued meanwhile run too).
fn run_all(level: &mut ServerLevel) {
    loop {
        let actions = QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if actions.is_empty() {
            return;
        }
        for action in actions {
            action(level);
        }
    }
}
