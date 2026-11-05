//! `JungleTempleStructure`.

use super::jungle_temple_piece::JungleTemplePiece;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::structure::single_piece_structure::SinglePieceStructure;
use mc_game_core::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings};
use linkme::distributed_slice;
use std::sync::Arc;

pub struct JungleTempleStructure {
    inner: SinglePieceStructure,
}

impl JungleTempleStructure {
    /// `JungleTempleStructure(settings)`.
    pub fn new(settings: StructureSettings) -> JungleTempleStructure {
        JungleTempleStructure { inner: SinglePieceStructure::new(|random, west, north| Box::new(JungleTemplePiece::new(random, west, north)), 12, 15, settings) }
    }
}

#[distributed_slice(STRUCTURE_TYPES)]
static JUNGLE_TEMPLE: TypeEntry<dyn Structure> = TypeEntry {
    name: "jungle_temple",
    decode: |fields, ctx| Ok(Arc::new(JungleTempleStructure::new(StructureSettings::decode(fields, ctx)?))),
};

impl Structure for JungleTempleStructure {
    fn settings(&self) -> &StructureSettings {
        &self.inner.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        self.inner.find_generation_point(context)
    }

    fn type_name(&self) -> &'static str {
        "jungle_temple"
    }
}
