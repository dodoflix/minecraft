//! `structure.pools`: the jigsaw system — template pools and their elements, junctions, dimension padding, pool aliases and
//! `JigsawPlacement` (the piece assembly of villages, bastions, ancient cities, trial chambers, ...).

pub mod empty_pool_element;
pub mod structure_pool_element;
pub mod structure_template_pool;

pub use structure_pool_element::StructurePoolElement;
pub use structure_template_pool::{Projection, StructureTemplatePool};
