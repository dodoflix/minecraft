//! `net.minecraft.world.level.material`: `Fluid` behavior trait + registry, `FluidState` dispatch extension.
//! The tick half of the concrete fluids (`FlowingFluid`, `WaterFluid`, `LavaFluid`) is implemented by the fluid slice and
//! registered with [`register_fluid!`]; `flowing_fluid` has the read-only half (`getFlow`, `getHeight`) as a fallback.
mod flowing_fluid;
mod fluid;
pub use flowing_fluid::FlowingFluid;
pub use fluid::*;
