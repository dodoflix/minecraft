//! `world.level.dimension.DimensionType`: the `dimension_type` registry (`DIRECT_CODEC` / `NETWORK_CODEC`).

use mc_core::ident::Identifier;
use mc_nbt::Tag;
use serde_json::Value;

use super::attributes::EnvironmentAttributeMap;
use super::int_provider::{self, IntProvider};
use super::{HolderSetRef, get_bool, get_f32, get_f64, get_i32, get_str};
use crate::registry::codec::{RegistryDecodeContext, Schema, default, opt, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `BlockPos.PACKED_Y_LENGTH`-derived limits of `DimensionType`.
pub const Y_SIZE: i32 = (1 << 12) - 32;
pub const MAX_Y: i32 = (Y_SIZE >> 1) - 1;
pub const MIN_Y: i32 = MAX_Y - Y_SIZE + 1;
pub const MIN_HEIGHT: i32 = 16;
/// `DimensionType.MOON_BRIGHTNESS_PER_PHASE`.
pub const MOON_BRIGHTNESS_PER_PHASE: [f32; 8] = [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75];

/// `DimensionType.Skybox`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Skybox {
    None,
    Overworld,
    End,
}

/// `CardinalLighting.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CardinalLightType {
    Default,
    Nether,
}

/// `CardinalLighting` per face (`down, up, north, south, west, east`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CardinalLighting {
    pub down: f32,
    pub up: f32,
    pub north: f32,
    pub south: f32,
    pub west: f32,
    pub east: f32,
}

impl CardinalLightType {
    pub fn get(self) -> CardinalLighting {
        match self {
            CardinalLightType::Default => CardinalLighting {
                down: 0.5,
                up: 1.0,
                north: 0.8,
                south: 0.8,
                west: 0.6,
                east: 0.6,
            },
            CardinalLightType::Nether => CardinalLighting {
                down: 0.9,
                up: 0.9,
                north: 0.8,
                south: 0.8,
                west: 0.6,
                east: 0.6,
            },
        }
    }
}

/// `DimensionType`.
#[derive(Clone, Debug)]
pub struct DimensionType {
    pub has_fixed_time: bool,
    pub has_sky_light: bool,
    pub has_ceiling: bool,
    pub has_ender_dragon_fight: bool,
    pub coordinate_scale: f64,
    pub min_y: i32,
    pub height: i32,
    pub logical_height: i32,
    /// `infiniburn`: a block tag or list of blocks.
    pub infiniburn: HolderSetRef,
    pub ambient_light: f32,
    /// `MonsterSettings.monsterSpawnLightTest` (`monster_spawn_light_level`).
    pub monster_spawn_light_test: IntProvider,
    pub monster_spawn_block_light_limit: i32,
    pub skybox: Skybox,
    pub cardinal_light_type: CardinalLightType,
    pub attributes: EnvironmentAttributeMap,
    pub timelines: HolderSetRef,
    /// `default_clock`: id of a `world_clock`.
    pub default_clock: Option<Identifier>,
}

impl DimensionType {
    pub fn has_end_flashes(&self) -> bool {
        self.skybox == Skybox::End
    }

    /// `getTeleportationScale(last, new)`.
    pub fn teleportation_scale(last: &DimensionType, new: &DimensionType) -> f64 {
        last.coordinate_scale / new.coordinate_scale
    }

    pub fn monster_spawn_light_test(&self) -> &IntProvider {
        &self.monster_spawn_light_test
    }

    /// `monsterSpawnBlockLightLimit()`.
    pub fn monster_spawn_block_light_limit(&self) -> i32 {
        self.monster_spawn_block_light_limit
    }

    /// `DIRECT_CODEC` decode.
    pub fn from_json(json: &Value, ctx: &RegistryDecodeContext) -> Result<DimensionType, String> {
        decode_checked(json, ctx, false).map(|(d, _)| d)
    }

    /// `DIRECT_CODEC` decode and re-encode (`encodeStart(NbtOps)`): the type and its canonical NBT.
    pub fn from_json_with_nbt(
        json: &Value,
        ctx: &RegistryDecodeContext,
    ) -> Result<(DimensionType, Tag), String> {
        decode_checked(json, ctx, false)
    }
}

fn record() -> Schema {
    Schema::Record(vec![
        default("has_fixed_time", Schema::Bool, Value::Bool(false)),
        req("has_skylight", Schema::Bool),
        req("has_ceiling", Schema::Bool),
        req("has_ender_dragon_fight", Schema::Bool),
        req(
            "coordinate_scale",
            Schema::DoubleRange(1.0E-5f32 as f64, 3.0E7),
        ),
        req("min_y", Schema::IntRange(MIN_Y, MAX_Y)),
        req("height", Schema::IntRange(16, Y_SIZE)),
        req("logical_height", Schema::IntRange(0, Y_SIZE)),
        req(
            "infiniburn",
            Schema::HolderSet {
                registry: keys::BLOCK,
                always_list: false,
            },
        ),
        req("ambient_light", Schema::Float),
        req("monster_spawn_light_level", int_provider::schema(0, 15)),
        req("monster_spawn_block_light_limit", Schema::IntRange(0, 15)),
        default(
            "skybox",
            Schema::Enum(&["none", "overworld", "end"]),
            Value::from("overworld"),
        ),
        default(
            "cardinal_light",
            Schema::Enum(&["default", "nether"]),
            Value::from("default"),
        ),
        // `attributes` is handled by `decode_checked`
        opt(
            "timelines",
            Schema::HolderSet {
                registry: keys::TIMELINE,
                always_list: false,
            },
        ),
        opt("default_clock", Schema::Holder(keys::WORLD_CLOCK)),
    ])
}

/// `DimensionType` constructor checks, in vanilla's order.
fn check(height: i32, min_y: i32, logical_height: i32) -> Result<(), String> {
    if height < MIN_HEIGHT {
        Err("height has to be at least 16".to_owned())
    } else if min_y + height > MAX_Y + 1 {
        Err(format!(
            "min_y + height cannot be higher than: {}",
            MAX_Y + 1
        ))
    } else if logical_height > height {
        Err("logical_height cannot be higher than height".to_owned())
    } else if height % 16 != 0 {
        Err("height has to be multiple of 16".to_owned())
    } else if min_y % 16 != 0 {
        Err("min_y has to be a multiple of 16".to_owned())
    } else {
        Ok(())
    }
}

fn decode_checked(
    json: &Value,
    ctx: &RegistryDecodeContext,
    network: bool,
) -> Result<(DimensionType, Tag), String> {
    let inner = || -> Result<(DimensionType, Tag), String> {
        let Tag::Compound(mut nbt) = record().convert(json, ctx)? else {
            return Err("Not a map".to_owned());
        };
        // `attributes`: optional, `EnvironmentAttributeMap.CODEC` (or `NETWORK_CODEC`)
        let attributes = match json.get("attributes") {
            Some(a) => {
                let (map, attr_nbt) = EnvironmentAttributeMap::parse(a, ctx, false, network)?;
                // `optionalFieldOf("attributes", EMPTY)` compares the value before the network codec's syncable filter
                if !map.is_empty() {
                    nbt.put("attributes", Tag::Compound(attr_nbt));
                }
                map
            }
            None => EnvironmentAttributeMap::default(),
        };
        let (min_y, height, logical_height) = (
            get_i32(json, "min_y").unwrap_or(0),
            get_i32(json, "height").unwrap_or(0),
            get_i32(json, "logical_height").unwrap_or(0),
        );
        check(height, min_y, logical_height)?;
        let dim = DimensionType {
            has_fixed_time: get_bool(json, "has_fixed_time").unwrap_or(false),
            has_sky_light: get_bool(json, "has_skylight").unwrap_or(false),
            has_ceiling: get_bool(json, "has_ceiling").unwrap_or(false),
            has_ender_dragon_fight: get_bool(json, "has_ender_dragon_fight").unwrap_or(false),
            coordinate_scale: get_f64(json, "coordinate_scale").unwrap_or(1.0),
            min_y,
            height,
            logical_height,
            infiniburn: json
                .get("infiniburn")
                .and_then(HolderSetRef::from_json)
                .ok_or("Missing infiniburn")?,
            ambient_light: get_f32(json, "ambient_light").unwrap_or(0.0),
            monster_spawn_light_test: json
                .get("monster_spawn_light_level")
                .and_then(IntProvider::from_json)
                .ok_or("Missing monster_spawn_light_level")?,
            monster_spawn_block_light_limit: get_i32(json, "monster_spawn_block_light_limit")
                .unwrap_or(0),
            skybox: match get_str(json, "skybox") {
                Some("none") => Skybox::None,
                Some("end") => Skybox::End,
                _ => Skybox::Overworld,
            },
            cardinal_light_type: if get_str(json, "cardinal_light") == Some("nether") {
                CardinalLightType::Nether
            } else {
                CardinalLightType::Default
            },
            attributes,
            timelines: json
                .get("timelines")
                .and_then(HolderSetRef::from_json)
                .unwrap_or(HolderSetRef::Elements(Vec::new())),
            default_clock: get_str(json, "default_clock").and_then(Identifier::try_parse),
        };
        Ok((dim, Tag::Compound(nbt)))
    };
    // `ExtraCodecs.catchDecoderException` wraps the constructor's `IllegalStateException`
    inner().map_err(|e| {
        if is_constructor_error(&e) {
            format!(
                "Caught exception decoding {}: {e}",
                serde_json::to_string(json).unwrap_or_default()
            )
        } else {
            e
        }
    })
}

fn is_constructor_error(e: &str) -> bool {
    matches!(
        e,
        "height has to be at least 16"
            | "logical_height cannot be higher than height"
            | "height has to be multiple of 16"
            | "min_y has to be a multiple of 16"
    ) || e.starts_with("min_y + height cannot be higher than")
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let (dim, _) = decode_checked(json, ctx, false)?;
    // `SYNCHRONIZED_REGISTRIES` sends `DimensionType.NETWORK_CODEC`
    let (_, network) = decode_checked(json, ctx, true)?;
    Ok(DecodedEntry::with_network(dim, network))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::DIMENSION_TYPE,
    decode,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::codec::{Lookup, RegistryLookup};
    use serde_json::json;

    struct L;
    impl RegistryLookup for L {
        fn contains(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
        fn tag_exists(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
    }

    fn base() -> Value {
        json!({
            "has_skylight": true, "has_ceiling": false, "has_ender_dragon_fight": false, "coordinate_scale": 1.0,
            "min_y": -64, "height": 384, "logical_height": 384, "infiniburn": "#minecraft:infiniburn_overworld",
            "ambient_light": 0.0, "monster_spawn_light_level": 7, "monster_spawn_block_light_limit": 0,
        })
    }

    #[test]
    fn decodes_and_checks() {
        let l = L;
        let ctx = RegistryDecodeContext::new(&l);
        let d = DimensionType::from_json(&base(), &ctx).unwrap();
        assert_eq!((d.min_y, d.height, d.logical_height), (-64, 384, 384));
        assert_eq!(d.monster_spawn_light_test, IntProvider::Constant(7));
        assert_eq!(d.skybox, Skybox::Overworld);
        let mut bad = base();
        bad["height"] = json!(100);
        bad["logical_height"] = json!(96);
        assert!(
            DimensionType::from_json(&bad, &ctx)
                .unwrap_err()
                .contains("height has to be multiple of 16")
        );
        let mut bad = base();
        bad["logical_height"] = json!(500);
        assert!(
            DimensionType::from_json(&bad, &ctx)
                .unwrap_err()
                .contains("logical_height cannot be higher than height")
        );
        let mut bad = base();
        bad["min_y"] = json!(-8);
        assert!(
            DimensionType::from_json(&bad, &ctx)
                .unwrap_err()
                .contains("min_y has to be a multiple of 16")
        );
        let mut bad = base();
        bad["height"] = json!(5000);
        assert_eq!(
            DimensionType::from_json(&bad, &ctx).unwrap_err(),
            "Value 5000 outside of range [16:4064]"
        );
    }
}
