//! `net.minecraft.world.level.material`: `Fluid` behavior trait + registry, `FluidState` dispatch extension.
//! The concrete fluids (`FlowingFluid`, `WaterFluid`, `LavaFluid`) are implemented by the fluid slice and registered
//! with [`register_fluid!`].
mod fluid;
pub use fluid::*;
