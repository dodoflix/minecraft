//! `SpawnContextLevel` of the live level.

use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;

use super::SpawnContextLevel;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;

impl SpawnContextLevel for ServerLevel {
    fn variant_registries(&self) -> &RegistryAccess {
        &self.registries
    }

    fn biome_at(&self, pos: BlockPos) -> Option<Identifier> {
        let id = self.get_biome_id(pos)?;
        Some(self.tick_state.biomes.as_ref()?.key(id).clone())
    }

    fn moon_brightness_at(&self, pos: BlockPos) -> f32 {
        self.get_moon_brightness(pos)
    }

    fn has_structure_piece_at(&self, pos: BlockPos, matcher: &dyn Fn(&Identifier) -> bool) -> bool {
        crate::advancements::predicates::location_predicate::has_structure_piece_at(self, pos, matcher)
    }
}
