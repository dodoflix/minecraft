//! The "please stop" flag shared between the OS signal handlers, the console and the game loop.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

/// Cloneable handle on one `AtomicBool`. SIGINT/SIGTERM/SIGHUP (vanilla: the JVM shutdown hook calling
/// `halt(true)`) set it; the server loop polls [`ServerStopSignal::is_stop_requested`] once per tick and then saves
/// and stops. The server also sets it when it stops for any other reason (the console reader exits with it).
#[derive(Clone, Default, Debug)]
pub struct ServerStopSignal(Arc<AtomicBool>);

impl ServerStopSignal {
    pub fn new() -> ServerStopSignal {
        ServerStopSignal::default()
    }

    pub fn request_stop(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_stop_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Make SIGINT, SIGTERM and SIGHUP request a stop (async-signal-safe: the handler only stores the flag).
    pub fn install_signal_handlers(&self) -> io::Result<()> {
        for signal in [SIGINT, SIGTERM, SIGHUP] {
            signal_hook::flag::register(signal, self.0.clone())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigterm_sets_flag() {
        let stop = ServerStopSignal::new();
        stop.install_signal_handlers().unwrap();
        assert!(!stop.is_stop_requested());
        signal_hook::low_level::raise(SIGTERM).unwrap();
        assert!(stop.is_stop_requested());
    }

    #[test]
    fn clones_share_state() {
        let a = ServerStopSignal::new();
        let b = a.clone();
        b.request_stop();
        assert!(a.is_stop_requested());
    }
}
