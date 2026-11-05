//! `structure.pools`: the jigsaw system — template pools and their elements, junctions, dimension padding, pool aliases and
//! `JigsawPlacement` (the piece assembly of villages, bastions, ancient cities, trial chambers, ...).

pub mod alias;
pub mod dimension_padding;
pub mod empty_pool_element;
pub mod feature_pool_element;
pub mod jigsaw_junction;
pub mod jigsaw_placement;
pub mod list_pool_element;
pub mod single_pool_element;
pub mod structure_pool_element;
pub mod structure_template_pool;

pub use dimension_padding::DimensionPadding;
pub use jigsaw_junction::JigsawJunction;
pub use structure_pool_element::StructurePoolElement;
pub use structure_template_pool::{Projection, StructureTemplatePool};
