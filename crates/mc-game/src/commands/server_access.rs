//! How a command handler reaches `&mut MinecraftServer`.
//!
//! Brigadier's `Command`/`SuggestionProvider`/`ArgumentType` closures only receive the `CommandContext`, and the source
//! stack is a cheap `Clone` value, so it cannot hold the server. vanilla's stack holds the `MinecraftServer`; here the
//! entry points that run commands ([`Commands::perform_command`](super::Commands), suggestions, tree building) install
//! a pointer to the server they were given for the duration of the call, and handlers borrow it with
//! [`with_server`]. The borrow is exclusive and checked at run time (the pointer is taken out while a handler holds it,
//! like a `RefCell`): nesting `with_server` is a bug and panics instead of aliasing.

use crate::server::MinecraftServer;
use std::cell::Cell;
use std::ptr::null_mut;

thread_local! {
    static SERVER: Cell<*mut MinecraftServer> = const { Cell::new(null_mut()) };
}

struct Restore(*mut MinecraftServer);

impl Drop for Restore {
    fn drop(&mut self) {
        SERVER.with(|s| s.set(self.0));
    }
}

/// Runs `f` with `server` reachable through [`with_server`] on this thread. `f` must not use `server` directly.
pub fn enter_server<R>(server: &mut MinecraftServer, f: impl FnOnce() -> R) -> R {
    let _restore = Restore(SERVER.with(|s| s.replace(server as *mut MinecraftServer)));
    f()
}

/// Whether a server was installed by [`enter_server`] (and is not currently borrowed).
pub fn has_server() -> bool {
    SERVER.with(|s| !s.get().is_null())
}

/// Borrows the installed server. Panics when called outside a command entry point or re-entrantly.
pub fn with_server<R>(f: impl FnOnce(&mut MinecraftServer) -> R) -> R {
    let ptr = SERVER.with(|s| s.replace(null_mut()));
    assert!(!ptr.is_null(), "with_server: no server installed (or already borrowed by an enclosing with_server)");
    let _restore = Restore(ptr);
    // SAFETY: `ptr` comes from the `&mut MinecraftServer` handed to `enter_server`, which is borrowed for the whole
    // call of `enter_server` and unused by it; the pointer was taken out of the cell, so no other `&mut` derived from it
    // can exist until `_restore` puts it back (also on unwind).
    f(unsafe { &mut *ptr })
}
