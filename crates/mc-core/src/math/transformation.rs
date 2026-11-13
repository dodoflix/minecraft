//! `com.mojang.math.Transformation`: an affine transformation held as a JOML `Matrix4f` and decomposed lazily into
//! `translation * leftRotation * scale * rightRotation` by `MatrixUtil.svdDecompose`.
//!
//! Not implemented: `slerp` (client-side display interpolation only).

use super::matrix_util::svd_decompose;
use super::matrix4f::Matrix4f;
use super::quaternionf::Quaternionf;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
struct Parts {
    translation: [f32; 3],
    left_rotation: Quaternionf,
    scale: [f32; 3],
    right_rotation: Quaternionf,
}

/// `Transformation`. Equality and hashing look at the matrix only, like vanilla.
#[derive(Clone, Debug)]
pub struct Transformation {
    matrix: Matrix4f,
    parts: OnceLock<Parts>,
    /// `this == IDENTITY` (reference equality in vanilla; `inverse` returns the shared instance).
    is_identity_constant: bool,
}

impl PartialEq for Transformation {
    fn eq(&self, other: &Self) -> bool {
        self.matrix == other.matrix
    }
}

impl Eq for Transformation {}

impl std::hash::Hash for Transformation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.matrix.hash(state);
    }
}

/// `Transformation.compose(translation, leftRotation, scale, rightRotation)` with absent parts skipped.
fn compose_matrix(
    translation: Option<[f32; 3]>,
    left_rotation: Option<&Quaternionf>,
    scale: Option<[f32; 3]>,
    right_rotation: Option<&Quaternionf>,
) -> Matrix4f {
    let mut result = Matrix4f::default();
    if let Some([x, y, z]) = translation {
        result.translation(x, y, z);
    }
    if let Some(q) = left_rotation {
        result.rotate(q);
    }
    if let Some([x, y, z]) = scale {
        result.scale(x, y, z);
    }
    if let Some(q) = right_rotation {
        result.rotate(q);
    }
    result
}

impl Transformation {
    /// `Transformation.IDENTITY`.
    pub fn identity() -> Self {
        let parts = OnceLock::new();
        let _ = parts.set(Parts {
            translation: [0.0; 3],
            left_rotation: Quaternionf::identity(),
            scale: [1.0; 3],
            right_rotation: Quaternionf::identity(),
        });
        Self {
            matrix: Matrix4f::default(),
            parts,
            is_identity_constant: true,
        }
    }

    /// `new Transformation(Matrix4fc)`: the parts are decomposed on first use.
    pub fn from_matrix(matrix: Matrix4f) -> Self {
        Self {
            matrix,
            parts: OnceLock::new(),
            is_identity_constant: false,
        }
    }

    /// `new Transformation(translation, leftRotation, scale, rightRotation)`; an absent part is the identity part.
    pub fn new(
        translation: Option<[f32; 3]>,
        left_rotation: Option<Quaternionf>,
        scale: Option<[f32; 3]>,
        right_rotation: Option<Quaternionf>,
    ) -> Self {
        let matrix = compose_matrix(
            translation,
            left_rotation.as_ref(),
            scale,
            right_rotation.as_ref(),
        );
        let parts = OnceLock::new();
        let _ = parts.set(Parts {
            translation: translation.unwrap_or([0.0; 3]),
            left_rotation: left_rotation.unwrap_or_default(),
            scale: scale.unwrap_or([1.0; 3]),
            right_rotation: right_rotation.unwrap_or_default(),
        });
        Self {
            matrix,
            parts,
            is_identity_constant: false,
        }
    }

    /// `compose(Transformation that)`: `this.matrix * that.matrix`.
    pub fn compose(&self, that: &Transformation) -> Transformation {
        let mut matrix = self.matrix.clone();
        matrix.mul(&that.matrix);
        Transformation::from_matrix(matrix)
    }

    /// `Transformation.compose(parent, Optional<Transformation>)`.
    pub fn compose_with_parent(parent: &Matrix4f, transform: Option<&Transformation>) -> Matrix4f {
        match transform {
            Some(t) => {
                let mut result = parent.clone();
                result.mul(&t.matrix);
                result
            }
            None => parent.clone(),
        }
    }

    /// `inverse()`: `None` when the affine inverse is not finite.
    pub fn inverse(&self) -> Option<Transformation> {
        if self.is_identity_constant {
            return Some(self.clone());
        }
        let mut matrix = self.matrix.clone();
        matrix.invert_affine();
        matrix
            .is_finite()
            .then(|| Transformation::from_matrix(matrix))
    }

    /// `getMatrix()`.
    pub fn matrix(&self) -> &Matrix4f {
        &self.matrix
    }

    fn parts(&self) -> &Parts {
        self.parts.get_or_init(|| {
            let (translation, left_rotation, scale, right_rotation) = svd_decompose(&self.matrix);
            Parts {
                translation,
                left_rotation,
                scale,
                right_rotation,
            }
        })
    }

    /// `translation()`.
    pub fn translation(&self) -> [f32; 3] {
        self.parts().translation
    }

    /// `leftRotation()`.
    pub fn left_rotation(&self) -> Quaternionf {
        self.parts().left_rotation
    }

    /// `scale()`.
    pub fn scale(&self) -> [f32; 3] {
        self.parts().scale
    }

    /// `rightRotation()`.
    pub fn right_rotation(&self) -> Quaternionf {
        self.parts().right_rotation
    }
}
