//! Entity types (`EntityType` builder data, spawn placement, default attributes).

use crate::enums::{HeightmapTypes, MobCategory, SpawnPlacementType};
use crate::ids::Attribute;

pub use crate::ids::EntityType;

/// `SpawnPlacements` registration of an entity type (the spawn predicate itself is code).
#[derive(Debug)]
pub struct SpawnPlacement {
    pub placement: SpawnPlacementType,
    pub heightmap: HeightmapTypes,
}

/// `MobCategory` constants.
#[derive(Debug)]
pub struct MobCategoryData {
    pub category: MobCategory,
    pub serialized_name: &'static str,
    pub max_instances_per_chunk: i32,
    pub friendly: bool,
    pub persistent: bool,
    pub despawn_distance: i32,
    pub no_despawn_distance: i32,
}

#[derive(Debug)]
pub struct EntityTypeData {
    pub name: &'static str,
    pub description_id: &'static str,
    pub category: MobCategory,
    /// `EntityDimensions` (float, as vanilla).
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub fixed: bool,
    /// `EntityAttachments` points (double) per attachment in order `PASSENGER, VEHICLE, NAME_TAG, WARDEN_CHEST`.
    pub attachments: [&'static [[f64; 3]]; 4],
    pub spawn_dimensions_scale: f32,
    pub client_tracking_range: i32,
    /// `Integer.MAX_VALUE` = no update interval.
    pub update_interval: i32,
    pub track_deltas: bool,
    pub fire_immune: bool,
    pub summonable: bool,
    pub serializable: bool,
    pub can_spawn_far_from_player: bool,
    pub allowed_in_peaceful: bool,
    /// Block tag the entity is immune to (`minecraft:default_immune_to`).
    pub immune_to: &'static str,
    pub loot_table: Option<&'static str>,
    pub required_features: &'static [&'static str],
    pub spawn_placement: Option<SpawnPlacement>,
    /// `DefaultAttributes` base values (only for living entities).
    pub default_attributes: &'static [(Attribute, f64)],
}

include!(concat!(env!("OUT_DIR"), "/entities.rs"));

/// Indices of [`EntityTypeData::attachments`].
pub mod attachment {
    pub const PASSENGER: usize = 0;
    pub const VEHICLE: usize = 1;
    pub const NAME_TAG: usize = 2;
    pub const WARDEN_CHEST: usize = 3;
}

impl EntityType {
    pub fn data(self) -> &'static EntityTypeData {
        &ENTITY_TYPE_DATA[self.0 as usize]
    }
}

impl MobCategory {
    pub fn data(self) -> &'static MobCategoryData {
        MOB_CATEGORIES
            .iter()
            .find(|d| d.category == self)
            .expect("every MobCategory has data")
    }
}
