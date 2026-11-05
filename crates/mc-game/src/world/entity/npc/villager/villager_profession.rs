//! `VillagerProfession` (a record of the built-in registry): the data attached to each `mc_data::VillagerProfession`.

use crate::world::entity::ai::village::poi::{PoiTypeHolder, PoiTypes};
use mc_core::ident::Identifier;
use mc_data::{Block, Item, SoundEvent, VillagerProfession};
use std::sync::OnceLock;

/// A `Predicate<Holder<PoiType>>` of a profession.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JobSitePredicate {
    /// `PoiType.NONE` (`poiType -> false`).
    Never,
    /// `poiType -> poiType.is(key)`.
    Is(&'static str),
    /// `VillagerProfession.ALL_ACQUIRABLE_JOBS` (`PoiTypeTags.ACQUIRABLE_JOB_SITE`).
    AllAcquirable,
}

impl JobSitePredicate {
    /// `test(holder)`.
    pub fn test(self, poi_type: PoiTypeHolder) -> bool {
        match self {
            JobSitePredicate::Never => false,
            JobSitePredicate::Is(key) => poi_type.is(key),
            JobSitePredicate::AllAcquirable => is_acquirable_job_site(poi_type),
        }
    }
}

/// `holder.is(PoiTypeTags.ACQUIRABLE_JOB_SITE)` (the tag of the vanilla data pack).
pub fn is_acquirable_job_site(poi_type: PoiTypeHolder) -> bool {
    static MASK: OnceLock<u64> = OnceLock::new();
    let mask = *MASK.get_or_init(|| {
        let names = mc_data::tags::names("point_of_interest_type", "minecraft:acquirable_job_site").unwrap_or_default();
        (0..mc_data::misc::POI_TYPES.len())
            .filter_map(PoiTypeHolder::from_index)
            .filter(|t| names.iter().any(|n| n == t.registered_name()))
            .fold(0u64, |m, t| m | 1 << t.index())
    });
    mask & (1 << poi_type.index()) != 0
}

/// The fields of the `VillagerProfession` record.
pub struct VillagerProfessionData {
    pub held_job_site: JobSitePredicate,
    pub acquirable_job_site: JobSitePredicate,
    pub requested_items: &'static [Item],
    pub secondary_poi: &'static [Block],
    pub work_sound: Option<SoundEvent>,
}

impl VillagerProfessionData {
    const fn job(poi: &'static str, work_sound: SoundEvent) -> Self {
        Self { held_job_site: JobSitePredicate::Is(poi), acquirable_job_site: JobSitePredicate::Is(poi), requested_items: &[], secondary_poi: &[], work_sound: Some(work_sound) }
    }
}

static NONE: VillagerProfessionData =
    VillagerProfessionData { held_job_site: JobSitePredicate::Never, acquirable_job_site: JobSitePredicate::AllAcquirable, requested_items: &[], secondary_poi: &[], work_sound: None };
static NITWIT: VillagerProfessionData =
    VillagerProfessionData { held_job_site: JobSitePredicate::Never, acquirable_job_site: JobSitePredicate::Never, requested_items: &[], secondary_poi: &[], work_sound: None };
static FARMER: VillagerProfessionData = VillagerProfessionData {
    held_job_site: JobSitePredicate::Is(PoiTypes::FARMER),
    acquirable_job_site: JobSitePredicate::Is(PoiTypes::FARMER),
    requested_items: &[Item::WHEAT, Item::WHEAT_SEEDS, Item::BEETROOT_SEEDS, Item::BONE_MEAL],
    secondary_poi: &[Block::FARMLAND],
    work_sound: Some(SoundEvent::ENTITY_VILLAGER_WORK_FARMER),
};
static ARMORER: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::ARMORER, SoundEvent::ENTITY_VILLAGER_WORK_ARMORER);
static BUTCHER: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::BUTCHER, SoundEvent::ENTITY_VILLAGER_WORK_BUTCHER);
static CARTOGRAPHER: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::CARTOGRAPHER, SoundEvent::ENTITY_VILLAGER_WORK_CARTOGRAPHER);
static CLERIC: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::CLERIC, SoundEvent::ENTITY_VILLAGER_WORK_CLERIC);
static FISHERMAN: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::FISHERMAN, SoundEvent::ENTITY_VILLAGER_WORK_FISHERMAN);
static FLETCHER: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::FLETCHER, SoundEvent::ENTITY_VILLAGER_WORK_FLETCHER);
static LEATHERWORKER: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::LEATHERWORKER, SoundEvent::ENTITY_VILLAGER_WORK_LEATHERWORKER);
static LIBRARIAN: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::LIBRARIAN, SoundEvent::ENTITY_VILLAGER_WORK_LIBRARIAN);
static MASON: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::MASON, SoundEvent::ENTITY_VILLAGER_WORK_MASON);
static SHEPHERD: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::SHEPHERD, SoundEvent::ENTITY_VILLAGER_WORK_SHEPHERD);
static TOOLSMITH: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::TOOLSMITH, SoundEvent::ENTITY_VILLAGER_WORK_TOOLSMITH);
static WEAPONSMITH: VillagerProfessionData = VillagerProfessionData::job(PoiTypes::WEAPONSMITH, SoundEvent::ENTITY_VILLAGER_WORK_WEAPONSMITH);

/// The record of a built-in profession.
pub fn data(profession: VillagerProfession) -> &'static VillagerProfessionData {
    match profession {
        VillagerProfession::NONE => &NONE,
        VillagerProfession::ARMORER => &ARMORER,
        VillagerProfession::BUTCHER => &BUTCHER,
        VillagerProfession::CARTOGRAPHER => &CARTOGRAPHER,
        VillagerProfession::CLERIC => &CLERIC,
        VillagerProfession::FARMER => &FARMER,
        VillagerProfession::FISHERMAN => &FISHERMAN,
        VillagerProfession::FLETCHER => &FLETCHER,
        VillagerProfession::LEATHERWORKER => &LEATHERWORKER,
        VillagerProfession::LIBRARIAN => &LIBRARIAN,
        VillagerProfession::MASON => &MASON,
        VillagerProfession::NITWIT => &NITWIT,
        VillagerProfession::SHEPHERD => &SHEPHERD,
        VillagerProfession::TOOLSMITH => &TOOLSMITH,
        VillagerProfession::WEAPONSMITH => &WEAPONSMITH,
        _ => &NITWIT,
    }
}

/// `profession.getTrades(level)`: the trade set key of that level (`None` for professions without trades).
pub fn get_trades(profession: VillagerProfession, level: i32) -> Option<Identifier> {
    crate::world::item::trading::trade_sets::profession_trade_set(&profession.identifier(), level)
}

/// `profession.name()`: the translation key `entity.minecraft.villager.<path>`.
pub fn name_key(profession: VillagerProfession) -> String {
    let id = profession.identifier();
    format!("entity.{}.villager.{}", id.namespace(), id.path())
}
