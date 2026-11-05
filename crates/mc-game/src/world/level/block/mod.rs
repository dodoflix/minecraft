//! `net.minecraft.world.level.block`: `BlockBehaviour` trait, per-block implementor lookup, `BlockState` dispatch
//! extension, `Block` helpers, concrete block classes.
mod behaviour;
pub mod helpers;
pub mod impls;
pub mod sculk_behaviour;
pub mod sculk_spreader;
pub mod state;
pub mod weathering_copper;

pub use behaviour::*;
pub use state::{BlockStateExt, UPDATE_SHAPE_ORDER};
