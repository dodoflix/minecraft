//! `net.minecraft.server.dedicated` + the process-level pieces of `net.minecraft.server.Main`
//! (command line, `eula.txt`, logging, console, stop signal).

pub mod eula;
pub mod java_properties;
pub mod options;
pub mod properties;
pub mod settings;
