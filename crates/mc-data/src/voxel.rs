//! `ShapeId` -> mc-core `VoxelShape`, plus the per-state face occlusion shapes (`BlockStateBase.getOcclusionShape`,
//! `getFaceOcclusionShape`) that the light engine and `ChunkSkyLightSources` consume.

use crate::block::BlockState;
use crate::shape::{ShapeData, ShapeId, ShapeKind, shape_count};
use mc_core::direction::Direction;
use mc_core::phys::shapes::{
    BitSetDiscreteVoxelShape, BooleanOp, Coords, DiscreteVoxelShape, Shapes, VoxelShape,
};
use std::sync::{Arc, OnceLock};

struct Cache {
    shapes: Vec<OnceLock<VoxelShape>>,
    faces: Vec<OnceLock<[VoxelShape; 6]>>,
}

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Cache {
        shapes: (0..shape_count()).map(|_| OnceLock::new()).collect(),
        faces: (0..shape_count()).map(|_| OnceLock::new()).collect(),
    })
}

fn build(data: &ShapeData) -> VoxelShape {
    let [xs, ys, zs] = data.size.map(i32::from);
    if xs == 0 || ys == 0 || zs == 0 {
        return Shapes::empty();
    }
    let words: Vec<u64> = data
        .bits
        .chunks(8)
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0u64, |w, (i, b)| w | u64::from(*b) << (8 * i))
        })
        .collect();
    let [fx, fy, fz] = data.first.map(i32::from);
    let [lx, ly, lz] = data.last.map(i32::from);
    let discrete: Arc<DiscreteVoxelShape> = Arc::new(
        BitSetDiscreteVoxelShape::from_raw(xs, ys, zs, &words, fx, fy, fz, lx, ly, lz).into(),
    );
    match data.kind {
        ShapeKind::Cube => {
            let shape = VoxelShape::cube(discrete);
            if xs == 1 && ys == 1 && zs == 1 && data.bits.first() == Some(&1) {
                Shapes::block()
            } else {
                shape
            }
        }
        ShapeKind::Array => {
            let [x, y, z] = data.coords.map(|c| Coords::array(c.to_vec()));
            VoxelShape::array(discrete, x, y, z)
        }
        ShapeKind::Slice => unreachable!("state shapes are never slices"),
    }
}

impl ShapeId {
    /// The shape as an mc-core `VoxelShape` (cached; `Shapes.empty()`/`Shapes.block()` keep their identity).
    pub fn voxel_shape(self) -> VoxelShape {
        cache().shapes[self.0 as usize]
            .get_or_init(|| build(self.data()))
            .clone()
    }
}

impl BlockState {
    /// `getOcclusionShape()`.
    pub fn occlusion_voxel_shape(self) -> VoxelShape {
        self.occlusion_shape().voxel_shape()
    }

    /// `getFaceOcclusionShape(direction)`: `occlusionShapesByFace` (empty / `Shapes.block()` x6 / `getFaceShape` per face).
    pub fn face_occlusion_shape(self, direction: Direction) -> VoxelShape {
        let id = self.occlusion_shape();
        let faces = cache().faces[id.0 as usize].get_or_init(|| {
            let shape = id.voxel_shape();
            if shape.is_empty() {
                std::array::from_fn(|_| Shapes::empty())
            } else if !Shapes::join_is_not_empty(&Shapes::block(), &shape, BooleanOp::NOT_SAME) {
                std::array::from_fn(|_| Shapes::block())
            } else {
                Direction::ALL.map(|d| shape.get_face_shape(d))
            }
        });
        faces[direction.ordinal()].clone()
    }
}
