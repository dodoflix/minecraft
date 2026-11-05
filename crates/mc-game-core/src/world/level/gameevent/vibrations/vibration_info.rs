//! `VibrationInfo`. The live `entity` reference is kept as the captured [`SourceEntityInfo`] (never saved).

use crate::server::level::ServerLevel;
use crate::world::level::gameevent::SourceEntityInfo;
use mc_core::phys::Vec3;
use mc_data::GameEvent;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DOUBLE, DataResult, float_range, record};
use mc_protocol::components::uuid_codec;
use std::sync::LazyLock;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct VibrationInfo {
    pub game_event: GameEvent,
    pub distance: f32,
    pub pos: Vec3,
    /// `uuid` (the source entity), saved as `source`.
    pub source: Option<Uuid>,
    pub projectile_owner: Option<Uuid>,
    /// `entity`: the source entity at the time of the event (`None` after loading from NBT).
    pub entity: Option<SourceEntityInfo>,
}

impl VibrationInfo {
    /// `VibrationInfo(gameEvent, distance, pos, entity)`.
    pub fn new(game_event: GameEvent, distance: f32, pos: Vec3, entity: Option<SourceEntityInfo>) -> Self {
        Self { game_event, distance, pos, source: entity.map(|e| e.uuid), projectile_owner: entity.and_then(|e| e.projectile_owner), entity }
    }

    /// `getEntity(level)`: the captured entity, else the entity with the saved UUID.
    pub fn get_entity(&self, level: &ServerLevel) -> Option<SourceEntityInfo> {
        self.entity.or_else(|| self.source.and_then(|uuid| level.get_entity_by_uuid(uuid)).map(|e| super::capture_source_entity(level, e)))
    }

    /// `getProjectileOwner(level)`: the owner of the source projectile, else the entity with the saved owner UUID.
    pub fn get_projectile_owner(&self, level: &ServerLevel) -> Option<SourceEntityInfo> {
        self.get_entity(level)
            .and_then(|e| e.projectile_owner)
            .or(self.projectile_owner)
            .and_then(|uuid| level.get_entity_by_uuid(uuid))
            .map(|e| super::capture_source_entity(level, e))
    }
}

/// `GameEvent.CODEC`: the registry name of the event (`holderByNameCodec`).
fn game_event_codec() -> Codec<GameEvent> {
    IDENTIFIER.flat_xmap(
        |id| match GameEvent::by_name(&id.to_string()) {
            Some(event) => DataResult::success(event),
            None => DataResult::error(move || format!("Unknown registry key in ResourceKey[minecraft:game_event / {id}]")),
        },
        |event: &GameEvent| DataResult::success(event.identifier()),
    )
}

/// `Vec3.CODEC`: a list of three doubles.
pub(crate) fn vec3_codec() -> Codec<Vec3> {
    DOUBLE.list_of().comap_flat_map(
        |list: Vec<f64>| match list[..] {
            [x, y, z] => DataResult::success(Vec3::new(x, y, z)),
            _ => DataResult::error_str("Input is not a list of 3 elements"),
        },
        |v: &Vec3| vec![v.x, v.y, v.z],
    )
}

/// `VibrationInfo.CODEC`.
pub static CODEC: LazyLock<Codec<VibrationInfo>> = LazyLock::new(|| {
    record!(Wire {
        game_event: game_event_codec().field_of("game_event"),
        distance: float_range(0.0, f32::MAX).field_of("distance"),
        pos: vec3_codec().field_of("pos"),
        source: uuid_codec().lenient_optional_field_of("source"),
        projectile_owner: uuid_codec().lenient_optional_field_of("projectile_owner"),
    })
    .codec()
    .xmap(
        |w: Wire| VibrationInfo { game_event: w.game_event, distance: w.distance, pos: w.pos, source: w.source, projectile_owner: w.projectile_owner, entity: None },
        |i: &VibrationInfo| Wire { game_event: i.game_event, distance: i.distance, pos: i.pos, source: i.source, projectile_owner: i.projectile_owner },
    )
});

struct Wire {
    game_event: GameEvent,
    distance: f32,
    pos: Vec3,
    source: Option<Uuid>,
    projectile_owner: Option<Uuid>,
}
