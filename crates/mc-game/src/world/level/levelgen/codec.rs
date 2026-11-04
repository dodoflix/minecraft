//! JSON codec helpers for the data-driven worldgen types: the `mc_worldgen::json` primitives plus the vanilla codecs
//! every feature/placement/structure codec shares (`BlockState`, `BlockPos`, holder sets of blocks/fluids, `type`
//! dispatch through a linkme type table).

pub use mc_worldgen::json::*;

use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, Vec3i};
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_worldgen::noise_settings::BlockStateSpec;
use serde_json::Value;
use std::sync::Arc;

/// One entry of a `registry.byNameCodec().dispatch(...)` table: the `minecraft:<name>` type and the decoder of its
/// `MapCodec` (it receives the whole object, `type` included). Tables are linkme slices declared next to their trait
/// (`FEATURE_TYPES`, `PLACEMENT_MODIFIER_TYPES`, ...); a type registers itself from its own file.
pub struct TypeEntry<T: ?Sized + 'static> {
    pub name: &'static str,
    pub decode: fn(&Fields<'_>, &crate::world::level::levelgen::registries::DecodeCtx<'_>) -> LoadResult<Arc<T>>,
}

/// `BuiltInRegistries.X.byNameCodec().dispatch(...)`: reads `type`, looks the type up in `table` and decodes.
pub fn decode_dispatch<T: ?Sized + 'static>(
    value: &Value,
    ctx: &crate::world::level::levelgen::registries::DecodeCtx<'_>,
    table: &[TypeEntry<T>],
) -> LoadResult<Arc<T>> {
    let fields = Fields::new(value)?;
    let ty = fields.required("type", parse_identifier)?;
    let entry = (ty.namespace() == "minecraft").then(|| table.iter().find(|e| e.name == ty.path())).flatten();
    match entry {
        Some(entry) => (entry.decode)(&fields, ctx),
        None => Err(LoadError::new(format!("Unknown registry key in dispatch: {ty}")).at("type")),
    }
}

/// `BlockState.CODEC` (`{Name?id, properties}` / bare id) -> the state.
pub fn parse_block_state(value: &Value) -> LoadResult<BlockState> {
    BlockStateSpec::from_json(value)?.resolve()
}

/// `BuiltInRegistries.BLOCK.byNameCodec()`.
pub fn parse_block(value: &Value) -> LoadResult<Block> {
    let id = parse_identifier(value)?;
    Block::by_name(&id.to_string()).ok_or_else(|| LoadError::new(format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:block]: {id}")))
}

/// `BuiltInRegistries.FLUID.byNameCodec()`.
pub fn parse_fluid(value: &Value) -> LoadResult<Fluid> {
    let id = parse_identifier(value)?;
    Fluid::by_name(&id.to_string()).ok_or_else(|| LoadError::new(format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:fluid]: {id}")))
}

/// `Vec3i.CODEC` / `BlockPos.CODEC`: a list of exactly three ints.
pub fn parse_block_pos(value: &Value) -> LoadResult<BlockPos> {
    let v = parse_list(value, parse_int)?;
    match v[..] {
        [x, y, z] => Ok(BlockPos::new(x, y, z)),
        _ => Err(LoadError::new("Input is not a list of 3 ints")),
    }
}

/// `Direction.CODEC` (lower-case serialized names).
pub fn parse_direction(value: &Value) -> LoadResult<Direction> {
    let name = parse_str(value)?;
    Direction::by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

/// `RegistryCodecs.homogeneousList(Registries.BLOCK)` (`HolderSet<Block>`): an id, a `#tag` or a list of ids.
#[derive(Clone, Debug)]
pub enum BlockSet {
    Blocks(Vec<Block>),
    /// A tag of the vanilla data pack, resolved at decode time (a missing tag decodes to an empty set like an unbound tag).
    Tag(Identifier, Option<&'static mc_data::tags::TagSet<Block>>),
}

impl BlockSet {
    pub fn from_json(value: &Value) -> LoadResult<BlockSet> {
        match value {
            Value::String(s) if s.starts_with('#') => {
                let tag = Identifier::read(&s[1..]).map_err(LoadError::new)?;
                let set = mc_data::tags::get::<Block>(&tag.to_string());
                Ok(BlockSet::Tag(tag, set))
            }
            Value::String(_) => Ok(BlockSet::Blocks(vec![parse_block(value)?])),
            Value::Array(_) => Ok(BlockSet::Blocks(parse_list(value, parse_block)?)),
            _ => Err(LoadError::new("Not a string or list")),
        }
    }

    pub fn contains(&self, block: Block) -> bool {
        match self {
            BlockSet::Blocks(blocks) => blocks.contains(&block),
            BlockSet::Tag(_, set) => set.is_some_and(|s| s.contains(block)),
        }
    }

    /// `HolderSet.contents()` in vanilla order (list order; tag file order with nested tags expanded in place):
    /// what `getRandomElement` indexes into.
    pub fn ordered_blocks(&self) -> Vec<Block> {
        match self {
            BlockSet::Blocks(blocks) => blocks.clone(),
            BlockSet::Tag(tag, _) => mc_data::tags::names("block", &tag.to_string())
                .map(|names| names.iter().filter_map(|n| Block::by_name(n)).collect())
                .unwrap_or_default(),
        }
    }
}

/// `HolderSet<Fluid>`: same shape as [`BlockSet`].
#[derive(Clone, Debug)]
pub enum FluidSet {
    Fluids(Vec<Fluid>),
    Tag(Identifier, Option<&'static mc_data::tags::TagSet<Fluid>>),
}

impl FluidSet {
    pub fn from_json(value: &Value) -> LoadResult<FluidSet> {
        match value {
            Value::String(s) if s.starts_with('#') => {
                let tag = Identifier::read(&s[1..]).map_err(LoadError::new)?;
                let set = mc_data::tags::get::<Fluid>(&tag.to_string());
                Ok(FluidSet::Tag(tag, set))
            }
            Value::String(_) => Ok(FluidSet::Fluids(vec![parse_fluid(value)?])),
            Value::Array(_) => Ok(FluidSet::Fluids(parse_list(value, parse_fluid)?)),
            _ => Err(LoadError::new("Not a string or list")),
        }
    }

    pub fn contains(&self, fluid: Fluid) -> bool {
        match self {
            FluidSet::Fluids(fluids) => fluids.contains(&fluid),
            FluidSet::Tag(_, set) => set.is_some_and(|s| s.contains(fluid)),
        }
    }
}

/// `Codec.LONG` (a number's `longValue()`: integers wrap to 64 bits, fractions truncate).
pub fn parse_long(value: &Value) -> LoadResult<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_u64().map(|v| v as i64))
            .or_else(|| {
                // beyond 2^116 an f64 integer is a multiple of 2^64 (BigDecimal.longValue keeps the low 64 bits: 0); `as i128` saturates at 2^127
                n.as_f64().map(|v| if v.abs() >= 2f64.powi(116) { 0 } else { v.trunc() as i128 as i64 })
            })
            .ok_or_else(|| LoadError::new("number out of range")),
        _ => Err(LoadError::new("expected number")),
    }
}

/// `Codec.floatRange(min, max)`: `Float.compareTo` bounds (so `-0.0` is below `0.0`).
pub fn parse_float_range(value: &Value, min: f32, max: f32) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if v.total_cmp(&min).is_ge() && v.total_cmp(&max).is_le() {
        Ok(v)
    } else {
        Err(LoadError::new(format!(
            "Value {} outside of range [{}:{}]",
            mc_core::math::java::java_float_to_string(v),
            mc_core::math::java::java_float_to_string(min),
            mc_core::math::java::java_float_to_string(max)
        )))
    }
}

/// `ExtraCodecs.POSITIVE_FLOAT`.
pub fn parse_positive_float(value: &Value) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if v.total_cmp(&0.0).is_gt() && v.total_cmp(&f32::MAX).is_le() {
        Ok(v)
    } else {
        Err(LoadError::new(format!("Value must be positive: {}", mc_core::math::java::java_float_to_string(v))))
    }
}

/// `Vec3i.CODEC`.
pub fn parse_vec3i(value: &Value) -> LoadResult<Vec3i> {
    parse_block_pos(value).map(Vec3i::from)
}

/// `Vec3i.offsetCodec(maxOffsetPerAxis)`.
pub fn parse_vec3i_offset(value: &Value, max_offset_per_axis: i32) -> LoadResult<Vec3i> {
    let v = parse_vec3i(value)?;
    // `Math.abs(Integer.MIN_VALUE)` stays negative in Java and therefore passes the check.
    if v.x.wrapping_abs() <= max_offset_per_axis && v.y.wrapping_abs() <= max_offset_per_axis && v.z.wrapping_abs() <= max_offset_per_axis {
        Ok(v)
    } else {
        Err(LoadError::new(format!("Position out of range, expected at most {max_offset_per_axis}: Vec3i{{x={}, y={}, z={}}}", v.x, v.y, v.z)))
    }
}
