//! `net.minecraft.world.level`.
pub mod base_command_block;
pub mod biome_ext;
pub mod blockscan;
pub mod block;
pub mod chunk;
pub mod collision;
pub mod entity;
pub mod explosion;
pub mod flags;
pub mod gameevent;
pub mod gamerules;
pub mod level_access;
pub mod levelgen;
pub mod material;
pub mod precipitation;
pub mod redstone;
pub mod saveddata;
pub mod signal_getter;
pub mod shapes;
pub mod signal_getter;
pub mod storage;
pub mod ticket_storage;

pub use level_access::{BlockGetter, LevelAccessor, LevelReader};
pub use mc_world::chunk::LevelHeight;

/// `ResourceKey<Level>` (the dimension id, e.g. `minecraft:overworld`).
pub type DimensionKey = mc_core::ident::Identifier;

/// `LightLayer`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightLayer {
    Sky,
    Block,
}
