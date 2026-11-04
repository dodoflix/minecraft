//! `net.minecraft.world.level`.
pub mod block;
pub mod flags;
pub mod gamerules;
pub mod level_access;
pub mod saveddata;

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
