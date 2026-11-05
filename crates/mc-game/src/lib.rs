//! All gameplay: `MinecraftServer`, player list, `ServerLevel`, chunk map, network listeners, blocks,
//! entities, items, commands, ... Module paths mirror vanilla packages (`net.minecraft.server.*` -> `server`,
//! `net.minecraft.world.*` -> `world`, `net.minecraft.commands` -> `commands`, registries/tags/holders ->
//! `registry`). See `ARCHITECTURE.md` ("mc-game design") for the threading/ownership contract.

pub mod advancements;
pub mod commands;
pub mod crash_report;
pub mod registry;
pub mod server;
pub mod util;
pub mod world;
pub mod stats;
