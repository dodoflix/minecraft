//! JSON codec helpers for the data-driven worldgen types: the `mc_worldgen::json` primitives plus the vanilla codecs
//! every feature/placement/structure codec shares (`BlockState`, `BlockPos`, holder sets of blocks/fluids, `type`
//! dispatch through a linkme type table).

pub use mc_worldgen::json::*;

use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
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

/// `Codec.floatRange(min, max)`.
pub fn parse_float_range(value: &Value, min: f32, max: f32) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if v >= min && v <= max {
        Ok(v)
    } else {
        let s = mc_core::math::java::java_float_to_string;
        Err(LoadError::new(format!("Value {} outside of range [{}:{}]", s(v), s(min), s(max))))
    }
}

/// `BlockState.CODEC` (`{Name?id, properties}` / bare id) -> the state.
pub fn parse_block_state(value: &Value) -> LoadResult<BlockState> {
    BlockStateSpec::from_json(value)?.resolve()
}

/// `Codec.floatRange(min, max)`.
pub fn parse_float_range(value: &Value, min: f32, max: f32) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if v >= min && v <= max {
        Ok(v)
    } else {
        Err(LoadError::new(format!("Value {v:?} outside of range [{min:?}:{max:?}]")))
    }
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
