//! NBT of a POI chunk (`SectionStorage.writeChunk` / `PackedChunk.parse` with `PoiSection.Packed.CODEC`):
//! `{DataVersion, Sections: {"<sectionY>": {Valid?: 1b, Records: [{pos: [I; x, y, z], type: "minecraft:home",
//! free_tickets?: int}]}}}`. `Valid` (when `false`) and `free_tickets` (when `0`) are not written (`optionalFieldOf`
//! with a default drops values equal to it).

use super::poi_record::PoiRecord;
use super::poi_type::PoiTypeHolder;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_nbt::{CompoundTag, ListTag, Tag};

pub const SECTIONS_TAG: &str = "Sections";
pub const DATA_VERSION_TAG: &str = "DataVersion";

/// `PoiRecord.Packed.CODEC` encode.
fn encode_record(record: &PoiRecord) -> CompoundTag {
    let pos = record.get_pos();
    let mut tag = CompoundTag::new();
    tag.put_int_array("pos", vec![pos.x(), pos.y(), pos.z()]);
    tag.put_string("type", record.get_poi_type().registered_name());
    if record.get_free_tickets() != 0 {
        tag.put_int("free_tickets", record.get_free_tickets());
    }
    tag
}

/// `PoiSection.Packed.CODEC` encode.
pub fn encode_section(is_valid: bool, records: &[PoiRecord]) -> CompoundTag {
    let mut tag = CompoundTag::new();
    if is_valid {
        tag.put_boolean("Valid", true);
    }
    let mut list = ListTag::new();
    for record in records {
        list.push(encode_record(record));
    }
    tag.put("Records", list);
    tag
}

/// `SectionStorage.writeChunk`: `Maps.newHashMap()` of the section tags (keyed by `Integer.toString(y)`) turned into a
/// compound in that map's iteration order, next to `DataVersion`.
pub fn encode_chunk(sections: Vec<(i32, CompoundTag)>, data_version: i32) -> CompoundTag {
    let mut hash_map = CompoundTag::new();
    for (y, section) in sections {
        hash_map.put(y.to_string(), section);
    }
    let mut sections_tag = CompoundTag::new();
    for (key, value) in hash_map.iter_java_order() {
        sections_tag.put(key.clone(), value.clone());
    }
    let mut root = CompoundTag::new();
    root.put(SECTIONS_TAG, sections_tag);
    root.put_int(DATA_VERSION_TAG, data_version);
    root
}

/// `Codec.INT` / `getNumberValue`: any numeric tag.
fn int_of(tag: &Tag) -> Option<i32> {
    tag.as_number().map(|n| n.int_value())
}

/// `BlockPos.CODEC`: an int stream of exactly three numbers (int/byte/long array or list of numbers).
fn decode_block_pos(tag: &Tag) -> Result<BlockPos, String> {
    let ints: Vec<i32> = match tag {
        Tag::IntArray(v) => v.clone(),
        Tag::ByteArray(v) => v.iter().map(|&b| i32::from(b)).collect(),
        Tag::LongArray(v) => v.iter().map(|&l| l as i32).collect(),
        Tag::List(list) => list
            .iter()
            .map(int_of)
            .collect::<Option<_>>()
            .ok_or("Not a list of numbers")?,
        _ => return Err("Not a list".to_owned()),
    };
    match ints[..] {
        [x, y, z] => Ok(BlockPos::new(x, y, z)),
        _ => Err(format!("Input is not a list of 3 ints, was {}", ints.len())),
    }
}

/// `PoiRecord.Packed.CODEC` decode.
fn decode_record(tag: &Tag) -> Result<PoiRecord, String> {
    let compound = tag.as_compound().ok_or("Not a map")?;
    let pos = decode_block_pos(compound.get("pos").ok_or("No key pos in MapLike")?)?;
    let type_name = compound
        .get("type")
        .ok_or("No key type in MapLike")?
        .as_string()
        .ok_or("Not a string")?;
    let identifier = Identifier::try_parse(type_name)
        .ok_or_else(|| format!("Not a valid resource location: {type_name}"))?;
    let poi_type = PoiTypeHolder::by_name(&identifier.to_string())
        .ok_or_else(|| format!("Unknown registry element {identifier}"))?;
    let free_tickets = match compound.get("free_tickets") {
        None => 0,
        Some(tag) => int_of(tag).ok_or("Not a number")?,
    };
    Ok(PoiRecord::with_free_tickets(pos, poi_type, free_tickets))
}

/// `PoiSection.Packed.CODEC` decode with `resultOrPartial(LOGGER::error)`: elements that fail to parse are dropped
/// (the partial list), a missing/invalid `Records` loses the section (`None`).
pub fn decode_section(tag: &Tag) -> Option<(bool, Vec<PoiRecord>)> {
    let compound = tag.as_compound()?;
    let is_valid = compound
        .get("Valid")
        .and_then(Tag::as_number)
        .is_some_and(|n| n.int_value() as i8 != 0);
    let records = match compound.get("Records") {
        Some(Tag::List(list)) => {
            let mut records = Vec::with_capacity(list.len());
            for element in list.iter() {
                match decode_record(element) {
                    Ok(record) => records.push(record),
                    Err(e) => log::error!("Failed to parse POI record: {e}"),
                }
            }
            records
        }
        Some(Tag::ByteArray(_) | Tag::IntArray(_) | Tag::LongArray(_)) => Vec::new(),
        _ => {
            log::error!("No key Records in MapLike");
            return None;
        }
    };
    Some((is_valid, records))
}

/// `PackedChunk.parse`: the decoded sections of `min_section_y..=max_section_y` by section y.
pub fn decode_chunk(
    tag: &CompoundTag,
    min_section_y: i32,
    max_section_y: i32,
) -> Vec<(i32, (bool, Vec<PoiRecord>))> {
    let Some(sections) = tag.get_compound(SECTIONS_TAG) else {
        return Vec::new();
    };
    (min_section_y..=max_section_y)
        .filter_map(|y| {
            sections
                .get(&y.to_string())
                .and_then(decode_section)
                .map(|section| (y, section))
        })
        .collect()
}
