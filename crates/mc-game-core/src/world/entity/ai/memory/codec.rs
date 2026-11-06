//! NBT form of memories: `ExpirableValue.codec(type codec)` = `{value: <by type>, ttl?: long}` and the
//! `Brain.Packed.CODEC` = `{memories: {<memory type id>: <expirable value>}}`.

use super::memory_module_type::{MemoryCodec, MemoryTypeId};
use super::value::{GlobalPosSet, MemoryValue, Unit};
use super::{ExpirableValue, MemoryMap};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::uuid_util::{uuid_from_int_array, uuid_to_int_array};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::codec::GlobalPos;

fn int_triple(tag: &Tag) -> Option<[i32; 3]> {
    // `Codec.INT_STREAM`: an int array or a list of numbers
    let ints: Vec<i32> = match tag {
        Tag::IntArray(a) => a.clone(),
        Tag::List(l) => l
            .iter()
            .map(|t| t.as_number().map(|n| n.int_value()))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    <[i32; 3]>::try_from(ints).ok()
}

fn uuid_of(tag: &Tag) -> Option<uuid::Uuid> {
    let ints: Vec<i32> = match tag {
        Tag::IntArray(a) => a.clone(),
        Tag::List(l) => l
            .iter()
            .map(|t| t.as_number().map(|n| n.int_value()))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    <[i32; 4]>::try_from(ints).ok().map(uuid_from_int_array)
}

fn block_pos_tag(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x(), pos.y(), pos.z()])
}

fn global_pos_tag(pos: &GlobalPos) -> Tag {
    let mut c = CompoundTag::new();
    c.put_string("dimension", pos.dimension.to_string());
    c.put("pos", block_pos_tag(pos.pos));
    Tag::Compound(c)
}

fn global_pos_of(tag: &Tag) -> Option<GlobalPos> {
    let c = tag.as_compound()?;
    let dimension = Identifier::try_parse(c.get_string("dimension")?)?;
    let [x, y, z] = int_triple(c.get("pos")?)?;
    Some(GlobalPos {
        dimension,
        pos: BlockPos::new(x, y, z),
    })
}

fn global_pos_list_tag<'a>(list: impl Iterator<Item = &'a GlobalPos>) -> Tag {
    let mut l = ListTag::new();
    for p in list {
        l.push(global_pos_tag(p));
    }
    Tag::List(l)
}

fn global_pos_list_of(tag: &Tag) -> Option<Vec<GlobalPos>> {
    tag.as_list()?.iter().map(global_pos_of).collect()
}

/// Encodes `value` with the codec of its memory type (`None` if the kinds do not match).
pub fn encode_value(codec: MemoryCodec, value: &MemoryValue) -> Option<Tag> {
    Some(match (codec, value) {
        (MemoryCodec::Bool, MemoryValue::Bool(v)) => Tag::Byte(i8::from(*v)),
        (MemoryCodec::Int, MemoryValue::Int(v)) => Tag::Int(*v),
        (MemoryCodec::Long, MemoryValue::Long(v)) => Tag::Long(*v),
        (MemoryCodec::Unit, MemoryValue::Unit(_)) => Tag::Compound(CompoundTag::new()),
        (MemoryCodec::GlobalPos, MemoryValue::GlobalPos(v)) => global_pos_tag(v),
        (MemoryCodec::BlockPos, MemoryValue::BlockPos(v)) => block_pos_tag(*v),
        (MemoryCodec::Uuid, MemoryValue::Uuid(v)) => Tag::IntArray(uuid_to_int_array(v).to_vec()),
        (MemoryCodec::GlobalPosList, MemoryValue::GlobalPosList(v)) => {
            global_pos_list_tag(v.iter())
        }
        (MemoryCodec::GlobalPosSet, MemoryValue::GlobalPosSet(v)) => global_pos_list_tag(v.iter()),
        _ => return None,
    })
}

/// Decodes a value with the codec of its memory type (`None` on a codec error).
pub fn decode_value(codec: MemoryCodec, tag: &Tag) -> Option<MemoryValue> {
    Some(match codec {
        MemoryCodec::None => return None,
        MemoryCodec::Bool => MemoryValue::Bool(tag.as_number()?.int_value() != 0),
        MemoryCodec::Int => MemoryValue::Int(tag.as_number()?.int_value()),
        MemoryCodec::Long => MemoryValue::Long(tag.as_number()?.long_value()),
        // `MapCodec.unitCodec` ignores its input
        MemoryCodec::Unit => MemoryValue::Unit(Unit),
        MemoryCodec::GlobalPos => MemoryValue::GlobalPos(global_pos_of(tag)?),
        MemoryCodec::BlockPos => {
            let [x, y, z] = int_triple(tag)?;
            MemoryValue::BlockPos(BlockPos::new(x, y, z))
        }
        MemoryCodec::Uuid => MemoryValue::Uuid(uuid_of(tag)?),
        MemoryCodec::GlobalPosList => MemoryValue::GlobalPosList(global_pos_list_of(tag)?),
        MemoryCodec::GlobalPosSet => MemoryValue::GlobalPosSet(
            global_pos_list_of(tag)?
                .into_iter()
                .collect::<GlobalPosSet>(),
        ),
    })
}

/// `ExpirableValue.codec(...)` encode; `None` for a type without a codec.
pub fn encode_expirable(type_id: MemoryTypeId, value: &ExpirableValue<MemoryValue>) -> Option<Tag> {
    let mut c = CompoundTag::new();
    c.put("value", encode_value(type_id.info().codec, &value.value)?);
    if let Some(ttl) = value.time_to_live {
        c.put_long("ttl", ttl);
    }
    Some(Tag::Compound(c))
}

/// `ExpirableValue.codec(...)` decode: `ttl` is a `lenientOptionalFieldOf` (an invalid `ttl` is ignored).
pub fn decode_expirable(type_id: MemoryTypeId, tag: &Tag) -> Option<ExpirableValue<MemoryValue>> {
    let c = tag.as_compound()?;
    let value = decode_value(type_id.info().codec, c.get("value")?)?;
    let time_to_live = c
        .get("ttl")
        .and_then(Tag::as_number)
        .map(|n| n.long_value());
    Some(ExpirableValue {
        value,
        time_to_live,
    })
}

impl MemoryMap {
    /// `MemoryMap.CODEC` encode: a compound keyed by memory type id.
    pub fn to_nbt(&self) -> CompoundTag {
        let mut out = CompoundTag::new();
        for (type_id, value) in self.iter() {
            if let Some(tag) = encode_expirable(type_id, value) {
                out.put(type_id.to_string(), tag);
            }
        }
        out
    }

    /// `MemoryMap.CODEC` decode. A failing entry (unknown or non-serializable type, malformed value) is dropped and
    /// the others are kept (the codec's partial result, which `ValueInput.read` uses).
    pub fn from_nbt(tag: &CompoundTag) -> MemoryMap {
        let mut out = MemoryMap::default();
        for (key, value) in tag.iter() {
            let Some(type_id) = Identifier::try_parse(key).and_then(|id| MemoryTypeId::by_id(&id))
            else {
                continue;
            };
            if !type_id.can_serialize() {
                continue;
            }
            if let Some(v) = decode_expirable(type_id, value) {
                out.insert(type_id, v);
            }
        }
        out
    }
}
