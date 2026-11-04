//! Port of `net.minecraft.world.phys.shapes` (voxel shapes). Collision contexts are out of scope.

mod boolean_op;
mod coords;
mod discrete;
#[allow(clippy::module_inception)]
mod shapes;
mod voxel_shape;

pub use boolean_op::BooleanOp;
pub use coords::{Coords, IndexMerger};
pub use discrete::{BitSet, BitSetDiscreteVoxelShape, DiscreteVoxelShape, SubShape};
pub use shapes::{AttachFace, Shapes};
pub use voxel_shape::VoxelShape;

/// Guava `DoubleMath.fuzzyEquals(a, b, tolerance)`.
pub(crate) fn fuzzy_equals(a: f64, b: f64, tolerance: f64) -> bool {
    assert!(tolerance >= 0.0, "tolerance ({tolerance}) must be >= 0");
    (a - b).abs() <= tolerance || a == b || (a.is_nan() && b.is_nan())
}
