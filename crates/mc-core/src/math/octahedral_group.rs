//! `com.mojang.math.OctahedralGroup` (the 48 symmetries of a cube) and `com.mojang.math.SymmetricGroup3`.
//!
//! Matrices (`transformation()`) are `[f32; 9]` in column-major order (`Matrix3f.get(float[9])` layout).

use std::fmt;
use std::sync::LazyLock;

use crate::direction::{Axis, Direction, FrontAndTop};

/// `SymmetricGroup3`: the six permutations of the axes (ordinal order as in vanilla).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymmetricGroup3 {
    P123,
    P213,
    P132,
    P312,
    P231,
    P321,
}

impl SymmetricGroup3 {
    pub const ALL: [SymmetricGroup3; 6] = [
        Self::P123,
        Self::P213,
        Self::P132,
        Self::P312,
        Self::P231,
        Self::P321,
    ];

    const fn perm(self) -> [usize; 3] {
        match self {
            Self::P123 => [0, 1, 2],
            Self::P213 => [1, 0, 2],
            Self::P132 => [0, 2, 1],
            Self::P312 => [2, 0, 1],
            Self::P231 => [1, 2, 0],
            Self::P321 => [2, 1, 0],
        }
    }

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `permute(int)`; panics unless `i` is 0, 1 or 2 (vanilla throws `IllegalArgumentException`).
    pub fn permute(self, i: usize) -> usize {
        assert!(i < 3, "Must be 0, 1 or 2, but got {i}");
        self.perm()[i]
    }

    /// `compose(that)` (`CAYLEY_TABLE`).
    pub fn compose(self, that: SymmetricGroup3) -> SymmetricGroup3 {
        SYM_CAYLEY[self.ordinal()][that.ordinal()]
    }

    /// `inverse()` (`INVERSE_TABLE`).
    pub fn inverse(self) -> SymmetricGroup3 {
        SYM_INVERSE[self.ordinal()]
    }

    pub fn permute_axis(self, axis: Axis) -> Axis {
        Axis::VALUES[self.permute(axis.ordinal())]
    }

    /// `permuteVector(Vector3i)` (returns the permuted copy).
    pub fn permute_vector(self, v: [i32; 3]) -> [i32; 3] {
        let p = self.perm();
        [v[p[0]], v[p[1]], v[p[2]]]
    }

    /// `permuteVector(Vector3f)` (returns the permuted copy).
    pub fn permute_vector_f32(self, v: [f32; 3]) -> [f32; 3] {
        let p = self.perm();
        [v[p[0]], v[p[1]], v[p[2]]]
    }

    /// `transformation()`: `new Matrix3f().zero().set(permute(i), i, 1.0F)`, column-major.
    pub fn transformation(self) -> [f32; 9] {
        let mut m = [0.0f32; 9];
        for i in 0..3 {
            m[self.perm()[i] * 3 + i] = 1.0;
        }
        m
    }
}

static SYM_CAYLEY: LazyLock<[[SymmetricGroup3; 6]; 6]> = LazyLock::new(|| {
    let mut table = [[SymmetricGroup3::P123; 6]; 6];
    for first in SymmetricGroup3::ALL {
        for second in SymmetricGroup3::ALL {
            let p = second.perm().map(|s| first.perm()[s]);
            table[first.ordinal()][second.ordinal()] = SymmetricGroup3::ALL
                .into_iter()
                .find(|c| c.perm() == p)
                .expect("closed under composition");
        }
    }
    table
});

static SYM_INVERSE: LazyLock<[SymmetricGroup3; 6]> = LazyLock::new(|| {
    SymmetricGroup3::ALL.map(|f| {
        SymmetricGroup3::ALL
            .into_iter()
            .find(|&s| f.compose(s) == SymmetricGroup3::P123)
            .unwrap()
    })
});

/// `OctahedralGroup`: the 48 axis permutation + per-axis inversion combinations, in vanilla ordinal order.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OctahedralGroup {
    Identity,
    Rot180FaceXy,
    Rot180FaceXz,
    Rot180FaceYz,
    Rot120Nnn,
    Rot120Nnp,
    Rot120Npn,
    Rot120Npp,
    Rot120Pnn,
    Rot120Pnp,
    Rot120Ppn,
    Rot120Ppp,
    Rot180EdgeXyNeg,
    Rot180EdgeXyPos,
    Rot180EdgeXzNeg,
    Rot180EdgeXzPos,
    Rot180EdgeYzNeg,
    Rot180EdgeYzPos,
    Rot90XNeg,
    Rot90XPos,
    Rot90YNeg,
    Rot90YPos,
    Rot90ZNeg,
    Rot90ZPos,
    Inversion,
    InvertX,
    InvertY,
    InvertZ,
    Rot60RefNnn,
    Rot60RefNnp,
    Rot60RefNpn,
    Rot60RefNpp,
    Rot60RefPnn,
    Rot60RefPnp,
    Rot60RefPpn,
    Rot60RefPpp,
    SwapXy,
    SwapYz,
    SwapXz,
    SwapNegXy,
    SwapNegYz,
    SwapNegXz,
    Rot90RefXNeg,
    Rot90RefXPos,
    Rot90RefYNeg,
    Rot90RefYPos,
    Rot90RefZNeg,
    Rot90RefZPos,
}

/// (serialized name, permutation, invertX, invertY, invertZ) by ordinal.
const DATA: [(&str, SymmetricGroup3, bool, bool, bool); 48] = [
    ("identity", SymmetricGroup3::P123, false, false, false),
    ("rot_180_face_xy", SymmetricGroup3::P123, true, true, false),
    ("rot_180_face_xz", SymmetricGroup3::P123, true, false, true),
    ("rot_180_face_yz", SymmetricGroup3::P123, false, true, true),
    ("rot_120_nnn", SymmetricGroup3::P231, false, false, false),
    ("rot_120_nnp", SymmetricGroup3::P312, true, false, true),
    ("rot_120_npn", SymmetricGroup3::P312, false, true, true),
    ("rot_120_npp", SymmetricGroup3::P231, true, false, true),
    ("rot_120_pnn", SymmetricGroup3::P312, true, true, false),
    ("rot_120_pnp", SymmetricGroup3::P231, true, true, false),
    ("rot_120_ppn", SymmetricGroup3::P231, false, true, true),
    ("rot_120_ppp", SymmetricGroup3::P312, false, false, false),
    (
        "rot_180_edge_xy_neg",
        SymmetricGroup3::P213,
        true,
        true,
        true,
    ),
    (
        "rot_180_edge_xy_pos",
        SymmetricGroup3::P213,
        false,
        false,
        true,
    ),
    (
        "rot_180_edge_xz_neg",
        SymmetricGroup3::P321,
        true,
        true,
        true,
    ),
    (
        "rot_180_edge_xz_pos",
        SymmetricGroup3::P321,
        false,
        true,
        false,
    ),
    (
        "rot_180_edge_yz_neg",
        SymmetricGroup3::P132,
        true,
        true,
        true,
    ),
    (
        "rot_180_edge_yz_pos",
        SymmetricGroup3::P132,
        true,
        false,
        false,
    ),
    ("rot_90_x_neg", SymmetricGroup3::P132, false, false, true),
    ("rot_90_x_pos", SymmetricGroup3::P132, false, true, false),
    ("rot_90_y_neg", SymmetricGroup3::P321, true, false, false),
    ("rot_90_y_pos", SymmetricGroup3::P321, false, false, true),
    ("rot_90_z_neg", SymmetricGroup3::P213, false, true, false),
    ("rot_90_z_pos", SymmetricGroup3::P213, true, false, false),
    ("inversion", SymmetricGroup3::P123, true, true, true),
    ("invert_x", SymmetricGroup3::P123, true, false, false),
    ("invert_y", SymmetricGroup3::P123, false, true, false),
    ("invert_z", SymmetricGroup3::P123, false, false, true),
    ("rot_60_ref_nnn", SymmetricGroup3::P312, true, true, true),
    ("rot_60_ref_nnp", SymmetricGroup3::P231, true, false, false),
    ("rot_60_ref_npn", SymmetricGroup3::P231, false, false, true),
    ("rot_60_ref_npp", SymmetricGroup3::P312, false, false, true),
    ("rot_60_ref_pnn", SymmetricGroup3::P231, false, true, false),
    ("rot_60_ref_pnp", SymmetricGroup3::P312, true, false, false),
    ("rot_60_ref_ppn", SymmetricGroup3::P312, false, true, false),
    ("rot_60_ref_ppp", SymmetricGroup3::P231, true, true, true),
    ("swap_xy", SymmetricGroup3::P213, false, false, false),
    ("swap_yz", SymmetricGroup3::P132, false, false, false),
    ("swap_xz", SymmetricGroup3::P321, false, false, false),
    ("swap_neg_xy", SymmetricGroup3::P213, true, true, false),
    ("swap_neg_yz", SymmetricGroup3::P132, false, true, true),
    ("swap_neg_xz", SymmetricGroup3::P321, true, false, true),
    ("rot_90_ref_x_neg", SymmetricGroup3::P132, true, false, true),
    ("rot_90_ref_x_pos", SymmetricGroup3::P132, true, true, false),
    ("rot_90_ref_y_neg", SymmetricGroup3::P321, true, true, false),
    ("rot_90_ref_y_pos", SymmetricGroup3::P321, false, true, true),
    ("rot_90_ref_z_neg", SymmetricGroup3::P213, false, true, true),
    ("rot_90_ref_z_pos", SymmetricGroup3::P213, true, false, true),
];

impl OctahedralGroup {
    pub const ALL: [OctahedralGroup; 48] = [
        Self::Identity,
        Self::Rot180FaceXy,
        Self::Rot180FaceXz,
        Self::Rot180FaceYz,
        Self::Rot120Nnn,
        Self::Rot120Nnp,
        Self::Rot120Npn,
        Self::Rot120Npp,
        Self::Rot120Pnn,
        Self::Rot120Pnp,
        Self::Rot120Ppn,
        Self::Rot120Ppp,
        Self::Rot180EdgeXyNeg,
        Self::Rot180EdgeXyPos,
        Self::Rot180EdgeXzNeg,
        Self::Rot180EdgeXzPos,
        Self::Rot180EdgeYzNeg,
        Self::Rot180EdgeYzPos,
        Self::Rot90XNeg,
        Self::Rot90XPos,
        Self::Rot90YNeg,
        Self::Rot90YPos,
        Self::Rot90ZNeg,
        Self::Rot90ZPos,
        Self::Inversion,
        Self::InvertX,
        Self::InvertY,
        Self::InvertZ,
        Self::Rot60RefNnn,
        Self::Rot60RefNnp,
        Self::Rot60RefNpn,
        Self::Rot60RefNpp,
        Self::Rot60RefPnn,
        Self::Rot60RefPnp,
        Self::Rot60RefPpn,
        Self::Rot60RefPpp,
        Self::SwapXy,
        Self::SwapYz,
        Self::SwapXz,
        Self::SwapNegXy,
        Self::SwapNegYz,
        Self::SwapNegXz,
        Self::Rot90RefXNeg,
        Self::Rot90RefXPos,
        Self::Rot90RefYNeg,
        Self::Rot90RefYPos,
        Self::Rot90RefZNeg,
        Self::Rot90RefZPos,
    ];

    pub const BLOCK_ROT_X_270: OctahedralGroup = OctahedralGroup::Rot90XPos;
    pub const BLOCK_ROT_X_180: OctahedralGroup = OctahedralGroup::Rot180FaceYz;
    pub const BLOCK_ROT_X_90: OctahedralGroup = OctahedralGroup::Rot90XNeg;
    pub const BLOCK_ROT_Y_270: OctahedralGroup = OctahedralGroup::Rot90YPos;
    pub const BLOCK_ROT_Y_180: OctahedralGroup = OctahedralGroup::Rot180FaceXz;
    pub const BLOCK_ROT_Y_90: OctahedralGroup = OctahedralGroup::Rot90YNeg;
    pub const BLOCK_ROT_Z_270: OctahedralGroup = OctahedralGroup::Rot90ZPos;
    pub const BLOCK_ROT_Z_180: OctahedralGroup = OctahedralGroup::Rot180FaceXy;
    pub const BLOCK_ROT_Z_90: OctahedralGroup = OctahedralGroup::Rot90ZNeg;

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `trace(invertX, invertY, invertZ, permutation)`: unique fingerprint in `0..48`.
    const fn trace_of(
        invert_x: bool,
        invert_y: bool,
        invert_z: bool,
        permutation: SymmetricGroup3,
    ) -> usize {
        let inversion_index = (if invert_z { 4 } else { 0 })
            + (if invert_y { 2 } else { 0 })
            + (if invert_x { 1 } else { 0 });
        permutation.ordinal() << 3 | inversion_index
    }

    fn trace(self) -> usize {
        let (_, p, x, y, z) = DATA[self.ordinal()];
        Self::trace_of(x, y, z, p)
    }

    /// `compose(that)` (`CAYLEY_TABLE`).
    pub fn compose(self, that: OctahedralGroup) -> OctahedralGroup {
        CAYLEY[self.ordinal()][that.ordinal()]
    }

    /// `inverse()` (`INVERSE_TABLE`).
    pub fn inverse(self) -> OctahedralGroup {
        INVERSE[self.ordinal()]
    }

    /// `transformation()`: `new Matrix3f().scaling(±1, ±1, ±1).mul(permutation.transformation())`, column-major.
    pub fn transformation(self) -> [f32; 9] {
        let (_, p, ix, iy, iz) = DATA[self.ordinal()];
        let s = [
            if ix { -1.0f32 } else { 1.0 },
            if iy { -1.0 } else { 1.0 },
            if iz { -1.0 } else { 1.0 },
        ];
        let r = p.transformation();
        // joml `Matrix3f.mul`: dest.mCR = fma(left.m0R, right.mC0, fma(left.m1R, right.mC1, left.m2R * right.mC2))
        let mut out = [0.0f32; 9];
        for c in 0..3 {
            for row in 0..3 {
                let left = |k: usize| if k == row { s[row] } else { 0.0f32 };
                out[c * 3 + row] = left(0).mul_add(
                    r[c * 3],
                    left(1).mul_add(r[c * 3 + 1], left(2) * r[c * 3 + 2]),
                );
            }
        }
        out
    }

    pub const fn get_serialized_name(self) -> &'static str {
        DATA[self.ordinal()].0
    }

    /// `CODEC` lookup by serialized name.
    pub fn by_name(name: &str) -> Option<OctahedralGroup> {
        Self::ALL
            .into_iter()
            .find(|g| g.get_serialized_name() == name)
    }

    /// `rotate(Direction)`
    pub fn rotate(self, direction: Direction) -> Direction {
        let old_axis = direction.get_axis();
        let old_direction = direction.get_axis_direction();
        let new_axis = self.permutation().inverse().permute_axis(old_axis);
        let new_direction = if self.inverts(new_axis) {
            old_direction.opposite()
        } else {
            old_direction
        };
        Direction::from_axis_and_direction(new_axis, new_direction)
    }

    /// `rotate(Vector3i)` (returns the rotated copy): permute, then negate the inverted axes.
    pub fn rotate_vec3i(self, v: [i32; 3]) -> [i32; 3] {
        let (_, p, ix, iy, iz) = DATA[self.ordinal()];
        let v = p.permute_vector(v);
        // `x * -1` wraps for `i32::MIN` like Java
        [
            v[0].wrapping_mul(if ix { -1 } else { 1 }),
            v[1].wrapping_mul(if iy { -1 } else { 1 }),
            v[2].wrapping_mul(if iz { -1 } else { 1 }),
        ]
    }

    pub const fn inverts(self, axis: Axis) -> bool {
        let (_, _, x, y, z) = DATA[self.ordinal()];
        match axis {
            Axis::X => x,
            Axis::Y => y,
            Axis::Z => z,
        }
    }

    pub const fn permutation(self) -> SymmetricGroup3 {
        DATA[self.ordinal()].1
    }

    /// `rotate(FrontAndTop)`: `None` where vanilla returns `null` (the rotated pair is not one of the 12
    /// `FrontAndTop` constants).
    pub fn rotate_front_and_top(self, input: FrontAndTop) -> Option<FrontAndTop> {
        FrontAndTop::from_front_and_top(self.rotate(input.front()), self.rotate(input.top()))
    }
}

impl fmt::Display for OctahedralGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_serialized_name())
    }
}

static CAYLEY: LazyLock<[[OctahedralGroup; 48]; 48]> = LazyLock::new(|| {
    let mut fingerprints = [OctahedralGroup::Identity; 48];
    for g in OctahedralGroup::ALL {
        fingerprints[g.trace()] = g;
    }
    let mut table = [[OctahedralGroup::Identity; 48]; 48];
    for first in OctahedralGroup::ALL {
        for second in OctahedralGroup::ALL {
            let composed_permutation = second.permutation().compose(first.permutation());
            let [ix, iy, iz] = Axis::VALUES
                .map(|a| first.inverts(a) ^ second.inverts(first.permutation().permute_axis(a)));
            table[first.ordinal()][second.ordinal()] =
                fingerprints[OctahedralGroup::trace_of(ix, iy, iz, composed_permutation)];
        }
    }
    table
});

static INVERSE: LazyLock<[OctahedralGroup; 48]> = LazyLock::new(|| {
    OctahedralGroup::ALL.map(|f| {
        OctahedralGroup::ALL
            .into_iter()
            .find(|&s| f.compose(s) == OctahedralGroup::Identity)
            .unwrap()
    })
});
