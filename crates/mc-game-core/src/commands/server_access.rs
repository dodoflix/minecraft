//! How a command handler reaches `&mut MinecraftServer`.
//!
//! Brigadier's `Command`/`SuggestionProvider`/`ArgumentType` closures only receive the `CommandContext`, and the source
//! stack is a cheap `Clone` value, so it cannot hold the server. vanilla's stack holds the `MinecraftServer`; here the
//! entry points that run commands ([`Commands::perform_command`](super::Commands), suggestions, tree building) install
//! a pointer to the server they were given for the duration of the call, and handlers borrow it with
//! [`with_server`]. The borrow is exclusive and checked at run time (the pointer is taken out while a handler holds it,
//! like a `RefCell`): nesting `with_server` is a bug and panics instead of aliasing.

use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::world::level::DimensionKey;
use std::cell::Cell;
use std::ptr::null_mut;

thread_local! {
    static SERVER: Cell<*mut MinecraftServer> = const { Cell::new(null_mut()) };
    /// The server as shared by [`share_server`]: what code that only holds a `&ServerLevel` (loot rolls) reads it through.
    static SHARED: Cell<*const MinecraftServer> = const { Cell::new(std::ptr::null()) };
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
    assert!(
        !ptr.is_null(),
        "with_server: no server installed (or already borrowed by an enclosing with_server)"
    );
    let _restore = Restore(ptr);
    // SAFETY: `ptr` comes from the `&mut MinecraftServer` handed to `enter_server`, which is borrowed for the whole
    // call of `enter_server` and unused by it; the pointer was taken out of the cell, so no other `&mut` derived from it
    // can exist until `_restore` puts it back (also on unwind).
    f(unsafe { &mut *ptr })
}

struct RestoreShared(*const MinecraftServer);

impl Drop for RestoreShared {
    fn drop(&mut self) {
        SHARED.with(|s| s.set(self.0));
    }
}

/// Runs `f` with `server` readable through [`with_server_ref`] while the caller keeps shared borrows of the server's levels
/// (a loot roll holds a `&ServerLevel` and may resolve text). The exclusive handle is taken out meanwhile, so [`with_server`]
/// panics instead of aliasing the shared borrow.
pub fn share_server<R>(server: &MinecraftServer, f: impl FnOnce() -> R) -> R {
    let _restore = Restore(SERVER.with(|s| s.replace(null_mut())));
    let _shared = RestoreShared(SHARED.with(|s| s.replace(server as *const MinecraftServer)));
    f()
}

/// Reads the server through the shared handle of [`share_server`], or through the installed one (borrowed shared for the
/// call and shared for nested calls). `None` when neither is available (no server, or a [`with_server`] borrow holds it).
pub fn with_server_ref_opt<R>(f: impl FnOnce(&MinecraftServer) -> R) -> Option<R> {
    let shared = SHARED.with(Cell::get);
    if !shared.is_null() {
        // SAFETY: set only by `share_server`, which borrows the server shared for the whole call of its closure.
        return Some(f(unsafe { &*shared }));
    }
    has_server().then(|| {
        with_server(|server| {
            let server: &MinecraftServer = server;
            share_server(server, || f(server))
        })
    })
}

/// [`with_server_ref_opt`] for command code: panics outside a command entry point or while [`with_server`] holds the server.
pub fn with_server_ref<R>(f: impl FnOnce(&MinecraftServer) -> R) -> R {
    with_server_ref_opt(f).expect(
        "with_server_ref: no server installed (or already borrowed by an enclosing with_server)",
    )
}

/// Runs `f` with the level of `dimension` out of `server` and the server installed again, for level code that loads block
/// entities (sign lines resolve text through the server, which finds the level through [`with_level_lent`]). `None` when the
/// server has no such level.
pub fn with_level_installed<R>(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    f: impl FnOnce(&mut ServerLevel) -> R,
) -> Option<R> {
    server.with_level(dimension, |server, level| enter_server(server, || f(level)))
}

/// Runs `f` for level code that resolves text (`ComponentUtils.resolve` finds entities through `server.level` and borrows the
/// server with [`with_server`]): `level` is lent to the installed server and the server is reachable again inside `f`, which
/// must not use `level`. `None` (nothing runs) when no server is installed, like a bare test level.
pub fn with_level_lent<R>(level: &mut ServerLevel, f: impl FnOnce() -> R) -> Option<R> {
    has_server().then(|| {
        with_server(|server| server.with_lent_level(level, |server| enter_server(server, f)))
    })
}
