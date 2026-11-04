//! `net.minecraft.server.dedicated` + the process-level pieces of `net.minecraft.server.Main`
//! (command line, `eula.txt`, logging, console, stop signal).

pub mod console;
pub mod dedicated_server;
pub mod eula;
pub mod java_properties;
pub mod launch;
pub mod logging;
pub mod options;
pub mod properties;
pub mod settings;
pub mod stop;

pub use launch::launch;
