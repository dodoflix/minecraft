//! Caps how many tests of one process run servers at the same time.
//!
//! `cargo test` runs a target's tests on as many threads as the machine has cores; a vanilla JVM or our server per
//! test then starves each other, and timing-sensitive checks (a bot walking, a server answering) become flaky. A test
//! takes one permit when it starts its first server and keeps it until its last server is dropped; its further servers
//! share that permit, so a test that needs two servers cannot deadlock against other tests.
//!
//! The limit is 2 tests; `MC_TESTKIT_TESTS=<n>` changes it.

use std::cell::RefCell;
use std::sync::{Arc, Condvar, LazyLock, Mutex, PoisonError, Weak};

struct Gate {
    free: Mutex<usize>,
    released: Condvar,
}

static GATE: LazyLock<Gate> = LazyLock::new(|| {
    let limit = std::env::var("MC_TESTKIT_TESTS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0).unwrap_or(2);
    Gate { free: Mutex::new(limit), released: Condvar::new() }
});

/// One test's right to run servers. Dropping the last handle lets the next test in.
pub(crate) struct Permit;

impl Drop for Permit {
    fn drop(&mut self) {
        *GATE.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        GATE.released.notify_one();
    }
}

thread_local! {
    /// The permit of the test running on this thread, if it holds one.
    static HELD: RefCell<Weak<Permit>> = const { RefCell::new(Weak::new()) };
}

/// The calling test's permit; waits for a free one if it has none yet.
pub(crate) fn acquire() -> Arc<Permit> {
    if let Some(held) = HELD.with(|h| h.borrow().upgrade()) {
        return held;
    }
    let mut free = GATE.free.lock().unwrap_or_else(PoisonError::into_inner);
    while *free == 0 {
        free = GATE.released.wait(free).unwrap_or_else(PoisonError::into_inner);
    }
    *free -= 1;
    drop(free);
    let permit = Arc::new(Permit);
    HELD.with(|h| *h.borrow_mut() = Arc::downgrade(&permit));
    permit
}
