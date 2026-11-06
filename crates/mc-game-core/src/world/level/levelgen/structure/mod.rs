//! Structures (`net.minecraft.world.level.levelgen.structure`): the piece API ([`piece`], [`pieces`]), `Structure`
//! definitions + types ([`structure`]), starts ([`structure_start`]) and the per-level registries/caches. Structure and
//! piece types register themselves (`STRUCTURE_TYPES`, `STRUCTURE_PIECE_TYPES`); the area owners add their
//! `pub mod` lines here.

pub mod beardifier;
pub mod check;
pub mod generation;
pub mod locate;
pub mod manager;
pub mod piece;
pub mod pieces;
pub mod pools;
pub mod registries;
pub mod structure;
pub mod structure_start;
pub mod templatesystem;

pub use manager::StructureManager;
pub use piece::{
    PieceBase, StructurePiece, StructurePieceSerializationContext, StructurePieceType,
};
pub use pieces::{PiecesContainer, StructurePiecesBuilder};
pub use structure::{GenerationContext, GenerationStub, Structure, StructureSettings};
pub use structure_start::{StartHandle, StructureStart};
