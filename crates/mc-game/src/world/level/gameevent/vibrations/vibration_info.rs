//! `VibrationInfo` (without the live `entity` reference, which is never saved).

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
    record!(VibrationInfo {
        game_event: game_event_codec().field_of("game_event"),
        distance: float_range(0.0, f32::MAX).field_of("distance"),
        pos: vec3_codec().field_of("pos"),
        source: uuid_codec().lenient_optional_field_of("source"),
        projectile_owner: uuid_codec().lenient_optional_field_of("projectile_owner"),
    })
    .codec()
});
