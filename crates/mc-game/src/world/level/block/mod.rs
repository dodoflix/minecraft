//! `net.minecraft.world.level.block`: `BlockBehaviour` trait, per-block implementor lookup, `BlockState` dispatch
//! extension, `Block` helpers, concrete block classes.
mod behaviour;
pub mod helpers;
pub mod state;

pub use behaviour::*;
pub use state::{BlockStateExt, UPDATE_SHAPE_ORDER};
