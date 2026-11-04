//! Raw voxel shape data, exactly as vanilla stores it internally: per-axis coordinate lists plus the
//! `DiscreteVoxelShape` (size and filled bits), and the merged `toAabbs()` boxes. mc-core builds `VoxelShape`s from this.

/// Which vanilla `VoxelShape` implementation held the shape (matters only for exact float behaviour of lookups).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// `CubeVoxelShape`: evenly divided coordinates (`i / size`).
    Cube,
    /// `ArrayVoxelShape`: explicit coordinate lists.
    Array,
    /// `SliceShape`.
    Slice,
}

#[derive(Debug)]
pub struct ShapeData {
    pub kind: ShapeKind,
    /// `DiscreteVoxelShape` x/y/z sizes (cell counts).
    pub size: [u8; 3],
    /// `firstFull(axis)` per axis.
    pub first: [u8; 3],
    /// `lastFull(axis)` per axis.
    pub last: [u8; 3],
    /// Filled cells, bit `(x * ySize + y) * zSize + z`, little-endian within bytes.
    pub bits: &'static [u8],
    /// `getCoords(X|Y|Z)`: `size + 1` cell boundaries each.
    pub coords: [&'static [f64]; 3],
    /// `toAabbs()`: `[minX, minY, minZ, maxX, maxY, maxZ]`.
    pub aabbs: &'static [[f64; 6]],
}

impl ShapeData {
    /// `DiscreteVoxelShape.isFull(x, y, z)`.
    pub fn is_full(&self, x: usize, y: usize, z: usize) -> bool {
        let [_, ys, zs] = self.size.map(usize::from);
        let i = (x * ys + y) * zs + z;
        self.bits.get(i >> 3).is_some_and(|b| b >> (i & 7) & 1 != 0)
    }

    /// `VoxelShape.isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|b| *b == 0)
    }
}

/// Index into the interned shape table; id 0 is `Shapes.empty()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeId(pub(crate) u16);

impl ShapeId {
    pub const EMPTY: ShapeId = ShapeId(0);

    pub const fn id(self) -> u32 {
        self.0 as u32
    }

    pub fn from_id(id: u32) -> Option<ShapeId> {
        ((id as usize) < SHAPES.len()).then_some(ShapeId(id as u16))
    }

    pub fn data(self) -> &'static ShapeData {
        &SHAPES[self.0 as usize]
    }

    pub fn is_empty(self) -> bool {
        self.data().is_empty()
    }
}

/// Number of interned shapes.
pub fn shape_count() -> usize {
    SHAPES.len()
}

include!(concat!(env!("OUT_DIR"), "/shapes.rs"));
