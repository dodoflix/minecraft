//! The narrow seam between raids and the village/POI storage (DimensionsScope.DimPoi): `ServerLevel::is_village`,
//! `isVillage(SectionPos)`, `PoiManager.getInRange(village, ..)` and `PoiManager.getRandom(home, ..)`. DimPoi installs its
//! implementation with [`set_raid_world`] once it is on main; until then no village is ever found.

use crate::server::level::ServerLevel;
use mc_core::pos::{BlockPos, SectionPos};
use mc_core::random::RandomSource;
use std::sync::OnceLock;

pub trait RaidWorld: Send + Sync {
    /// `level.isVillage(pos)`.
    fn is_village(&self, level: &ServerLevel, pos: BlockPos) -> bool;
    /// `level.isVillage(sectionPos)`.
    fn is_village_section(&self, level: &ServerLevel, section: SectionPos) -> bool {
        self.is_village(level, section.center())
    }
    /// Positions of `level.getPoiManager().getInRange(t -> t.is(PoiTypeTags.VILLAGE), center, radius, IS_OCCUPIED)` in vanilla order.
    fn occupied_village_pois(&self, level: &ServerLevel, center: BlockPos, radius: i32) -> Vec<BlockPos>;
    /// `getPoiManager().getRandom(t -> t.is(PoiTypes.HOME), filter, ANY, center, radius, random)`.
    fn random_home_poi(&self, level: &ServerLevel, filter: &dyn Fn(BlockPos) -> bool, center: BlockPos, radius: i32, random: &mut dyn RandomSource) -> Option<BlockPos>;
}

static RAID_WORLD: OnceLock<&'static dyn RaidWorld> = OnceLock::new();

pub fn set_raid_world(world: &'static dyn RaidWorld) {
    let _ = RAID_WORLD.set(world);
}

pub fn is_village(level: &ServerLevel, pos: BlockPos) -> bool {
    RAID_WORLD.get().is_some_and(|w| w.is_village(level, pos))
}

pub fn is_village_section(level: &ServerLevel, section: SectionPos) -> bool {
    RAID_WORLD.get().is_some_and(|w| w.is_village_section(level, section))
}

pub fn occupied_village_pois(level: &ServerLevel, center: BlockPos, radius: i32) -> Vec<BlockPos> {
    RAID_WORLD.get().map(|w| w.occupied_village_pois(level, center, radius)).unwrap_or_default()
}

pub fn random_home_poi(level: &ServerLevel, filter: &dyn Fn(BlockPos) -> bool, center: BlockPos, radius: i32, random: &mut dyn RandomSource) -> Option<BlockPos> {
    RAID_WORLD.get().and_then(|w| w.random_home_poi(level, filter, center, radius, random))
}
