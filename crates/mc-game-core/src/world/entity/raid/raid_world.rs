//! The village/POI queries of raids (`ServerLevel::isVillage`, `PoiManager.getInRange(village, ..)`,
//! `PoiManager.getRandom(home, ..)`) over the level's POI manager.

use crate::server::level::ServerLevel;
use crate::world::entity::ai::village::poi::{Occupancy, PoiTypeHolder, PoiTypes};
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, SectionPos};
use mc_core::random::RandomSource;

/// `level.isVillage(pos)`.
pub fn is_village(level: &ServerLevel, pos: BlockPos) -> bool {
    level.is_village(pos)
}

/// `level.isVillage(sectionPos)`.
pub fn is_village_section(level: &ServerLevel, section: SectionPos) -> bool {
    level.is_village_section(section)
}

/// Positions of `getPoiManager().getInRange(t -> t.is(PoiTypeTags.VILLAGE), center, radius, IS_OCCUPIED)` in vanilla order.
pub fn occupied_village_pois(level: &ServerLevel, center: BlockPos, radius: i32) -> Vec<BlockPos> {
    let village = Identifier::parse("minecraft:village").expect("static id");
    let in_village = |t: PoiTypeHolder| Identifier::parse(t.registered_name()).is_ok_and(|id| level.registries.is_in_tag("minecraft:point_of_interest_type", &id, &village));
    level.poi_manager().get_in_range(&in_village, center, radius, Occupancy::IsOccupied).iter().map(|r| r.get_pos()).collect()
}

/// `getPoiManager().getRandom(t -> t.is(PoiTypes.HOME), filter, ANY, center, radius, random)`.
pub fn random_home_poi(level: &ServerLevel, filter: &dyn Fn(BlockPos) -> bool, center: BlockPos, radius: i32, random: &mut dyn RandomSource) -> Option<BlockPos> {
    level.poi_manager().get_random(&|t| t.is(PoiTypes::HOME), filter, Occupancy::Any, center, radius, random)
}
