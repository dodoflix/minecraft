//! Wiring of `mc_data` shape ids to mc-core [`VoxelShape`]s, plus the `Block` shape helpers
//! (`Block.isShapeFullBlock`, `Block.isFaceFull`, `Block.box`, ...).
//!
//! Vanilla compares shapes by identity in several places (`shape == Shapes.block()`); the cache therefore maps
//! every id whose geometry is the full cube to the interned `Shapes.block()` and id 0 to `Shapes.empty()`.

use mc_core::direction::Direction;
use mc_core::phys::shapes::{
    BitSetDiscreteVoxelShape, BooleanOp, Coords, DiscreteVoxelShape, Shapes, VoxelShape,
};
use mc_data::shape::{ShapeData, ShapeId, ShapeKind, shape_count};
use std::sync::{Arc, OnceLock};

fn build(data: &ShapeData) -> VoxelShape {
    let [xs, ys, zs] = data.size.map(i32::from);
    let words: Vec<u64> = data
        .bits
        .chunks(8)
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0u64, |acc, (i, b)| acc | u64::from(*b) << (8 * i))
        })
        .collect();
    let discrete = DiscreteVoxelShape::BitSet(BitSetDiscreteVoxelShape::from_raw(
        xs,
        ys,
        zs,
        &words,
        i32::from(data.first[0]),
        i32::from(data.first[1]),
        i32::from(data.first[2]),
        i32::from(data.last[0]),
        i32::from(data.last[1]),
        i32::from(data.last[2]),
    ));
    match data.kind {
        ShapeKind::Cube => VoxelShape::cube(discrete),
        ShapeKind::Array | ShapeKind::Slice => {
            let coords = data.coords.map(|c| Coords::array(Arc::<[f64]>::from(c)));
            let [cx, cy, cz] = coords;
            VoxelShape::array(discrete, cx, cy, cz)
        }
    }
}

/// The `VoxelShape` of an extracted shape id (cached; the full cube is `Shapes.block()`, id 0 `Shapes.empty()`).
pub fn voxel_shape(id: ShapeId) -> VoxelShape {
    static CACHE: OnceLock<Vec<OnceLock<VoxelShape>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| (0..shape_count()).map(|_| OnceLock::new()).collect());
    cache[id.id() as usize]
        .get_or_init(|| {
            if id == ShapeId::EMPTY {
                return Shapes::empty();
            }
            let shape = build(id.data());
            if is_shape_full_block(&shape) {
                Shapes::block()
            } else {
                shape
            }
        })
        .clone()
}

/// `Block.isShapeFullBlock`: `!Shapes.joinIsNotEmpty(Shapes.block(), shape, NOT_SAME)`.
pub fn is_shape_full_block(shape: &VoxelShape) -> bool {
    !Shapes::join_is_not_empty(&Shapes::block(), shape, BooleanOp::NOT_SAME)
}

/// `Block.isFaceFull(shape, direction)`.
pub fn is_face_full(shape: &VoxelShape, direction: Direction) -> bool {
    is_shape_full_block(&shape.get_face_shape(direction))
}

/// `Block.box(minX, minY, minZ, maxX, maxY, maxZ)` (pixel units).
pub fn box_(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> VoxelShape {
    Shapes::box_(
        min_x / 16.0,
        min_y / 16.0,
        min_z / 16.0,
        max_x / 16.0,
        max_y / 16.0,
        max_z / 16.0,
    )
}

/// `Block.column(sizeXZ, minY, maxY)`.
pub fn column(size_xz: f64, min_y: f64, max_y: f64) -> VoxelShape {
    column_xz(size_xz, size_xz, min_y, max_y)
}

/// `Block.column(sizeX, sizeZ, minY, maxY)`.
pub fn column_xz(size_x: f64, size_z: f64, min_y: f64, max_y: f64) -> VoxelShape {
    let (half_x, half_z) = (size_x / 2.0, size_z / 2.0);
    box_(
        8.0 - half_x,
        min_y,
        8.0 - half_z,
        8.0 + half_x,
        max_y,
        8.0 + half_z,
    )
}

/// `Block.cube(size)`.
pub fn cube(size: f64) -> VoxelShape {
    cube_xyz(size, size, size)
}

/// `Block.cube(sizeX, sizeY, sizeZ)`.
pub fn cube_xyz(size_x: f64, size_y: f64, size_z: f64) -> VoxelShape {
    let half_y = size_y / 2.0;
    column_xz(size_x, size_z, 8.0 - half_y, 8.0 + half_y)
}

/// `Block.boxZ(sizeX, minY, maxY, minZ, maxZ)`.
pub fn box_z(size_x: f64, min_y: f64, max_y: f64, min_z: f64, max_z: f64) -> VoxelShape {
    let half_x = size_x / 2.0;
    box_(8.0 - half_x, min_y, min_z, 8.0 + half_x, max_y, max_z)
}

/// `Block.boxZ(sizeXY, minZ, maxZ)`.
pub fn box_z_square(size_xy: f64, min_z: f64, max_z: f64) -> VoxelShape {
    box_z_xy(size_xy, size_xy, min_z, max_z)
}

/// `Block.boxZ(sizeX, sizeY, minZ, maxZ)`.
pub fn box_z_xy(size_x: f64, size_y: f64, min_z: f64, max_z: f64) -> VoxelShape {
    let half_y = size_y / 2.0;
    box_z(size_x, 8.0 - half_y, 8.0 + half_y, min_z, max_z)
}
