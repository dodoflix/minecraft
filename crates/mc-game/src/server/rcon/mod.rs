//! `net.minecraft.server.rcon`: the remote console (`RconThread`/`RconClient`) and the GS4 query listener
//! (`QueryThreadGs4`), as plain std threads. The game thread owns command execution: the server hands in an
//! [`RconCommandRunner`] that queues the command to the game thread and blocks for its output (`ServerInterface.runCommand`).

mod pkt_utils;
mod query;
mod rcon_thread;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;

pub use pkt_utils::{NetworkDataOutputStream, MAX_PACKET_SIZE, int_from_byte_array, int_from_network_byte_array, string_from_byte_array, to_hex_string};
pub use query::{QueryServerInfo, QueryThreadGs4};
pub use rcon_thread::{RconClient, RconThread};

/// `ServerInterface.runCommand`: run `cmd` as the rcon console source on the game thread and return its output.
pub trait RconCommandRunner: Send + Sync {
    fn run(&self, cmd: &str) -> String;
}

/// `GenericThread`: a named worker with a cooperative `running` flag.
pub(crate) struct GenericThread {
    name: String,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

static UNIQUE_THREAD_ID: AtomicUsize = AtomicUsize::new(0);

impl GenericThread {
    pub(crate) fn new(name: impl Into<String>) -> GenericThread {
        GenericThread { name: name.into(), running: Arc::new(AtomicBool::new(false)), thread: None }
    }

    pub(crate) fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// `start`: spawn `<name> #<n>` running `body(running_flag)`. Logs `Thread <name> started`.
    pub(crate) fn start(&mut self, body: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> bool {
        if self.is_running() {
            return true;
        }
        self.running.store(true, Ordering::SeqCst);
        let flag = self.running.clone();
        let thread_name = format!("{} #{}", self.name, UNIQUE_THREAD_ID.fetch_add(1, Ordering::SeqCst) + 1);
        match std::thread::Builder::new().name(thread_name).spawn(move || body(flag)) {
            Ok(handle) => {
                self.thread = Some(handle);
                log::info!("Thread {} started", self.name);
                true
            }
            Err(e) => {
                self.running.store(false, Ordering::SeqCst);
                log::warn!("Failed to start thread {}\n{e}", self.name);
                false
            }
        }
    }

    /// `stop`: clear the flag and wait for the thread to exit.
    pub(crate) fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
            log::info!("Thread {} stopped", self.name);
        }
    }
}
