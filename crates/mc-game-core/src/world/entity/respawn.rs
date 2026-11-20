//! `ServerPlayer.RespawnConfig`, `RespawnPosAngle`, `SavedPosition` and the respawn-position search
//! (`findRespawnAndUseSpawnBlock` joins once `ServerLevel` implements `BlockGetter`).

use crate::world::block_entity::read_map_codec;
use crate::world::item::enchantment::codecs::{vec2_codec, vec3_codec};
use crate::world::level::DimensionKey;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::BlockPos;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::RespawnData;
use std::sync::LazyLock;

/// `ServerPlayer.RespawnConfig(respawnData, forced)`.
#[derive(Clone, Debug, PartialEq)]
pub struct RespawnConfig {
    pub respawn_data: RespawnData,
    pub forced: bool,
}

impl RespawnConfig {
    /// `RespawnConfig.CODEC` decode (the `RespawnData` map codec keys sit next to `forced`).
    pub fn decode(tag: &Tag) -> Result<RespawnConfig, String> {
        let respawn_data = RespawnData::decode(tag)?;
        // `Codec.BOOL.optionalFieldOf("forced", false)`: a present but non-boolean value is an error
        let forced = match tag.as_compound().and_then(|c| c.get("forced")) {
            None => false,
            Some(t) => t.as_boolean().ok_or("Not a boolean")?,
        };
        Ok(RespawnConfig {
            respawn_data,
            forced,
        })
    }

    pub fn encode(&self) -> Tag {
        let Tag::Compound(mut c) = self.respawn_data.encode() else {
            unreachable!("RespawnData encodes to a compound")
        };
        if self.forced {
            c.put_boolean("forced", true);
        }
        Tag::Compound(c)
    }

    /// `getDimensionOrDefault(respawnConfig)`.
    pub fn dimension_or_default(config: Option<&RespawnConfig>) -> DimensionKey {
        config.map_or_else(
            || Identifier::with_default_namespace("overworld").expect("valid"),
            |c| c.respawn_data.dimension.clone(),
        )
    }

    /// `isSamePosition`: same `GlobalPos` (dimension + block).
    pub fn is_same_position(&self, other: Option<&RespawnConfig>) -> bool {
        other.is_some_and(|o| {
            self.respawn_data.dimension == o.respawn_data.dimension
                && self.respawn_data.pos == o.respawn_data.pos
        })
    }
}

/// `RespawnData.of(dimension, pos, yaw, pitch)` (wraps the yaw, clamps the pitch).
pub fn respawn_data_of(
    dimension: DimensionKey,
    pos: BlockPos,
    yaw: f32,
    pitch: f32,
) -> RespawnData {
    RespawnData {
        dimension,
        pos,
        yaw: mth::wrap_degrees_f32(yaw),
        pitch: mth::clamp_f32(pitch, -90.0, 90.0),
    }
}

/// `ServerPlayer.SavedPosition`: the position part of a player file, read before the player exists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedPosition {
    pub dimension: Option<DimensionKey>,
    pub position: Option<Vec3>,
    pub rotation: Option<Vec2>,
}

impl SavedPosition {
    pub const EMPTY: SavedPosition = SavedPosition {
        dimension: None,
        position: None,
        rotation: None,
    };

    /// `SavedPosition.MAP_CODEC`.
    fn map_codec() -> &'static mc_dfu::MapCodec<SavedPosition> {
        static CODEC: LazyLock<mc_dfu::MapCodec<SavedPosition>> = LazyLock::new(|| {
            mc_dfu::record!(SavedPosition {
                dimension: mc_dfu::extra::IDENTIFIER.optional_field_of("Dimension"),
                position: vec3_codec().optional_field_of("Pos"),
                rotation: vec2_codec().optional_field_of("Rotation"),
            })
        });
        &CODEC
    }

    /// `tag.read(MAP_CODEC).orElse(EMPTY)`: a failed decode keeps its partial value, and is reported to the open
    /// [`problem_reporter`](crate::world::block_entity::problem_reporter) scope.
    pub fn read(tag: &CompoundTag) -> SavedPosition {
        read_map_codec(tag, Self::map_codec()).unwrap_or(Self::EMPTY)
    }
}

/// `ServerPlayer.RespawnPosAngle`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RespawnPosAngle {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl RespawnPosAngle {
    /// `RespawnPosAngle.of(position, lookAtBlockPos, pitch)`.
    pub fn of(position: Vec3, look_at_block_pos: BlockPos, pitch: f32) -> Self {
        Self {
            position,
            yaw: calculate_look_at_yaw(position, look_at_block_pos),
            pitch,
        }
    }
}

fn calculate_look_at_yaw(position: Vec3, look_at: BlockPos) -> f32 {
    let target = Vec3::new(
        f64::from(look_at.x()) + 0.5,
        f64::from(look_at.y()),
        f64::from(look_at.z()) + 0.5,
    );
    let look = target.subtract(position).normalize();
    mth::wrap_degrees_f64(mth::atan2(look.z, look.x) * f64::from(mth::RAD_TO_DEG) - 90.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_and_forced_default() {
        let data = respawn_data_of(
            Identifier::try_parse("minecraft:the_nether").unwrap(),
            BlockPos::new(1, 2, 3),
            190.0,
            100.0,
        );
        assert_eq!((data.yaw, data.pitch), (-170.0, 90.0));
        let cfg = RespawnConfig {
            respawn_data: data,
            forced: false,
        };
        let tag = cfg.encode();
        assert!(tag.as_compound().unwrap().get("forced").is_none());
        assert_eq!(RespawnConfig::decode(&tag).unwrap(), cfg);
        let forced = RespawnConfig {
            forced: true,
            ..cfg
        };
        assert_eq!(RespawnConfig::decode(&forced.encode()).unwrap(), forced);
    }

    #[test]
    fn saved_position_keeps_the_fields_that_decode() {
        let mut c = CompoundTag::new();
        c.put_string("Dimension", "minecraft:overworld");
        c.put(
            "Pos",
            Tag::List(
                [Tag::double(1.5), Tag::double(64.0), Tag::double(-3.0)]
                    .into_iter()
                    .collect(),
            ),
        );
        let p = SavedPosition::read(&c);
        assert_eq!(p.position, Some(Vec3::new(1.5, 64.0, -3.0)));
        assert!(p.dimension.is_some() && p.rotation.is_none());
        // a field that fails to decode is left out, the others are kept (`OptionalFieldCodec` returns a partial result)
        c.put(
            "Rotation",
            Tag::List([Tag::float(1.0)].into_iter().collect()),
        );
        let partial = SavedPosition::read(&c);
        assert_eq!(partial.position, Some(Vec3::new(1.5, 64.0, -3.0)));
        assert!(partial.dimension.is_some() && partial.rotation.is_none());
    }
}
