//! `net.minecraft.world.level.entity.*`: section-based entity storage and the persistent entity manager.
//!
//! Vanilla keeps `Entity` references in sections/lookups and notifies the level through `LevelCallback`/
//! `EntityInLevelCallback`. Here the manager *owns* the entities (`Box<dyn Entity>`), sections store ids, and the
//! callbacks become [`EntityLevelEvent`]s the level drains right after each manager operation.
mod manager;
mod section;
mod section_storage;
mod tick_list;
mod visibility;

pub use manager::{ChunkLoadStatus, EntityLevelEvent, PersistentEntitySectionManager};
pub use section::EntitySection;
pub use section_storage::EntitySectionStorage;
pub use tick_list::EntityTickList;
pub use visibility::Visibility;
