//! `net.minecraft.world.entity.ai.village.poi`: points of interest (beds, job sites, bells, nether portals, ...), their
//! per-section storage and the `poi/` region files.

pub mod poi_codec;
pub mod poi_manager;
pub mod poi_record;
pub mod poi_section;
pub mod poi_type;

pub use poi_manager::PoiManager;
pub use poi_record::PoiRecord;
pub use poi_section::{Occupancy, PoiSection};
pub use poi_type::{PoiTypeHolder, PoiTypes};
