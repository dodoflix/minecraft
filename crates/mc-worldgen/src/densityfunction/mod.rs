//! net.minecraft.world.level.levelgen.densityfunction: the declarative density function tree
//! ([`DensityFunction`]), its compiled form ([`sampler::DensitySampler`]) and the optimizing compiler.

pub mod codec;
pub mod compiler;
pub mod cubic_spline;
pub mod function;
pub mod generator;
pub mod op;
pub mod sampler;

pub use compiler::{DensityFunctionCompiler, DfRewriteRule, SliceUniformAxes, inline_reference};
pub use function::{
    ALL_AXES, AXIS_X, AXIS_Y, AXIS_Z, CompileContext, DensityFunction, DensityFunctionHolder,
    DfNode, DfOp, HolderHolder, NO_AXES, NoiseHolder, PreparedCache, axes_from,
};
pub use sampler::{
    Blender, ContextKey, ContextMap, DensityBuffer, DensitySampler, DensityVolume, SamplerContext,
    ScopedDensityBuffer, SharedSampler,
};

/// `Direction.Axis` (kept local to the density function engine; `choose` picks the matching coordinate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// `Axis.choose(x, y, z)`.
    #[inline]
    pub fn choose(self, x: i32, y: i32, z: i32) -> i32 {
        match self {
            Axis::X => x,
            Axis::Y => y,
            Axis::Z => z,
        }
    }
    /// Serialized name ("x", "y", "z").
    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
            Axis::Z => "z",
        }
    }
    pub fn from_name(name: &str) -> Option<Axis> {
        match name {
            "x" => Some(Axis::X),
            "y" => Some(Axis::Y),
            "z" => Some(Axis::Z),
            _ => None,
        }
    }
}
