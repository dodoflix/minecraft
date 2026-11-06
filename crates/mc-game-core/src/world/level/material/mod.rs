//! `net.minecraft.world.level.material`: `Fluid` behavior trait + registry, `FluidState` dispatch extension and the
//! concrete fluids (`EmptyFluid`, `FlowingFluid`, `WaterFluid`, `LavaFluid`), registered with [`register_fluid!`].
pub mod flowing_fluid;
mod fluid;
pub mod lava_fluid;
pub mod water_fluid;
pub use fluid::*;
pub mod map_color;
