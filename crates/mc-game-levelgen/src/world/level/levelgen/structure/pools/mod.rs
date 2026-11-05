// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::levelgen::structure::pools::*;

pub mod alias;

pub mod dimension_padding;

pub use dimension_padding::DimensionPadding;

pub mod feature_pool_element;

pub mod jigsaw_junction;

pub use jigsaw_junction::JigsawJunction;

pub mod jigsaw_placement;

pub mod list_pool_element;

pub mod single_pool_element;
