// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::levelgen::structure::*;
pub mod pools;
pub mod templatesystem;

pub mod pool_element_structure_piece;

pub mod scattered_feature_piece;

pub mod single_piece_structure;

pub mod structures;

pub mod template_structure_piece;
