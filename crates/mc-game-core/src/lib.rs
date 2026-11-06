//! `mc-game-core`: the gameplay spine (`MinecraftServer`, player list, `ServerLevel`, chunk map, network listeners and the
//! base traits for blocks, entities, items and commands); the area crates `mc-game-*` implement on top of it. Module
//! paths mirror vanilla packages (`net.minecraft.server.*` -> `server`, `net.minecraft.world.*` -> `world`, ...). See
//! `ARCHITECTURE.md` ("mc-game design") for the threading/ownership contract.

pub mod advancements;
pub mod commands;
pub mod crash_report;
pub mod gametest;
pub mod registry;
pub mod server;
pub mod stats;
pub mod util;
pub mod world;
