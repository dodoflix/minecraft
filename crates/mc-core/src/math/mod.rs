//! Math helpers: `java` (Java numeric/string library semantics), `mth` (vanilla `net.minecraft.util.Mth`),
//! `octahedral_group` (`com.mojang.math.OctahedralGroup` / `SymmetricGroup3`) and the display transformation stack:
//! `matrix4f`/`matrix3f`/`quaternionf` (JOML float arithmetic), `matrix_util` (`MatrixUtil` SVD) and `transformation`.
mod atan_tables;
pub mod java;
mod joml;
pub mod matrix3f;
pub mod matrix4f;
pub mod matrix_util;
pub mod mth;
pub mod octahedral_group;
pub mod quaternionf;
pub mod transformation;

pub use joml::rotate_y;
