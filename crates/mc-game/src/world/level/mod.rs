//! `net.minecraft.world.level`.
pub mod block;
pub mod collision;
pub mod entity;
pub mod flags;
pub mod gameevent;
pub mod gamerules;
pub mod level_access;
pub mod material;
pub mod precipitation;
pub mod redstone;
pub mod saveddata;
pub mod shapes;

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
