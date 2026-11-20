//! `LegacyStructureFileFix`: the structure starts and references that old worlds kept in `data/<Structure>.dat` index
//! files move into the chunks they belong to (`Level.Structures`).

use mc_core::ident::Identifier;
use mc_core::java_collections::JavaHashMap;
use mc_core::math::java::java_string_hash;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::{DataFixTypes, References};
use mc_dfu::{Dynamic, NBT};
use mc_nbt::CompoundTag;
use std::collections::BTreeMap;

use crate::filefix::{
    ChunkNbt, ContentContext, FileFix, FileFixError, FileRelation, MissingSeverity, Status, Type,
    only_file,
};
use crate::region::RegionStorageInfo;
use crate::region::upgrade::chunk_data_fix_context_tag;

pub const STRUCTURE_RANGE: i32 = 8;
pub const OVERWORLD_LEGACY_STRUCTURES: [&str; 5] =
    ["Monument", "Stronghold", "Mineshaft", "Temple", "Mansion"];
pub const NETHER_LEGACY_STRUCTURES: [&str; 1] = ["Fortress"];
pub const END_LEGACY_STRUCTURES: [&str; 1] = ["EndCity"];

/// `LEGACY_TO_CURRENT_MAP.get(id)`.
fn legacy_to_current(id: &str) -> Option<&'static str> {
    match id {
        "Iglu" => Some("Igloo"),
        "TeDP" => Some("Desert_Pyramid"),
        "TeJP" => Some("Jungle_Pyramid"),
        "TeSH" => Some("Swamp_Hut"),
        _ => None,
    }
}

/// `LegacyStructureData`: the structure starts of one chunk and the chunk positions of the starts it references.
struct LegacyStructureData {
    starts: JavaHashMap<String, Dynamic>,
    indexes: JavaHashMap<String, Vec<i64>>,
}

impl LegacyStructureData {
    fn new() -> LegacyStructureData {
        LegacyStructureData {
            starts: JavaHashMap::new(|k: &String| java_string_hash(k)),
            indexes: JavaHashMap::new(|k: &String| java_string_hash(k)),
        }
    }

    fn add_start(&mut self, id: String, data: Dynamic) {
        self.starts.insert(id, data);
    }

    fn add_index(&mut self, id: &str, source_pos: i64) {
        self.indexes
            .compute_if_absent(id.to_owned(), |_| Vec::new());
        self.indexes
            .get_mut(&id.to_owned())
            .expect("just inserted")
            .push(source_pos);
    }
}

/// One dimension: its index files, its chunks and the data of the chunks that get structures.
struct DimensionFixEntry {
    dimension: &'static str,
    structure_ids: &'static [&'static str],
    old_folder: FileRelation,
    structures: BTreeMap<i64, LegacyStructureData>,
}

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    fix.add_file_content_fix(|ctx| {
        let level_dat = only_file(ctx.level_dat(&FileRelation::origin().for_file("level.dat")))?;
        let Some(level_data) = level_dat.read(ctx.tree)? else {
            return Ok(());
        };
        ctx.progress.set_type(Type::LegacyStructures);
        let mut entries = [
            DimensionFixEntry {
                dimension: "minecraft:overworld",
                structure_ids: &OVERWORLD_LEGACY_STRUCTURES,
                old_folder: FileRelation::old_overworld(),
                structures: BTreeMap::new(),
            },
            DimensionFixEntry {
                dimension: "minecraft:the_nether",
                structure_ids: &NETHER_LEGACY_STRUCTURES,
                old_folder: FileRelation::old_nether(),
                structures: BTreeMap::new(),
            },
            DimensionFixEntry {
                dimension: "minecraft:the_end",
                structure_ids: &END_LEGACY_STRUCTURES,
                old_folder: FileRelation::old_end(),
                structures: BTreeMap::new(),
            },
        ];
        extract_and_store_legacy_structure_data(ctx, &level_data, &mut entries)
    });
    fix
}

/// `extractAndStoreLegacyStructureData`.
fn extract_and_store_legacy_structure_data(
    ctx: &mut ContentContext<'_>,
    level_data: &Dynamic,
    entries: &mut [DimensionFixEntry],
) -> Result<(), FileFixError> {
    ctx.progress.set_status(Status::Counting);
    for entry in entries.iter_mut() {
        for structure_id in entry.structure_ids {
            let target_file = only_file(ctx.saved_data(
                References::SAVED_DATA_STRUCTURE_FEATURE_INDICES,
                &FileRelation::data().for_file(&format!("{structure_id}.dat")),
                MissingSeverity::Minor,
            ))?;
            if let Some(structure_data) = target_file.read(ctx.tree)? {
                extract_legacy_structure_data(&structure_data, &mut entry.structures);
            }
        }
        ctx.progress
            .add_total_file_fix_operations(entry.structures.len() as i32);
    }

    ctx.progress.set_status(Status::Upgrading);
    for entry in entries.iter() {
        let info = RegionStorageInfo::new(
            entry.dimension.trim_start_matches("minecraft:"),
            Identifier::parse(entry.dimension).expect("a dimension id"),
            "chunk",
        );
        let mut chunk_nbt = only_file(ctx.chunk_nbt(
            &entry.old_folder.resolve(&FileRelation::region()),
            &info,
            DataFixTypes::CHUNK,
        )?)?;
        let chunk_generator_type = if entry.dimension == "minecraft:overworld" {
            match level_data
                .get("generatorName")
                .as_string_or("buffet")
                .as_str()
            {
                "flat" => "minecraft:flat",
                "debug_all_block_states" => "minecraft:debug",
                _ => "minecraft:noise",
            }
        } else {
            "minecraft:noise"
        };
        let data_fix_context =
            chunk_data_fix_context_tag(entry.dimension, Some(chunk_generator_type));
        let stored = store_legacy_structure_data_to_chunks(
            &entry.structures,
            &mut chunk_nbt,
            ctx,
            &data_fix_context,
        );
        // closed on every path, as the file accesses of `ModifyContent` are
        let closed = chunk_nbt.close(ctx.tree);
        stored?;
        closed?;
    }
    Ok(())
}

/// `extractLegacyStructureData`: the starts of an index file, and the neighbours that reference them.
fn extract_legacy_structure_data(
    structure_data: &Dynamic,
    extracted: &mut BTreeMap<i64, LegacyStructureData>,
) {
    let features = structure_data.get("Features");
    let map: Vec<(Dynamic, Dynamic)> = features.as_map(|key| key, |value| value);
    for (_, mut value) in map {
        let pos = ChunkPos::pack_xz(
            value.get("ChunkX").as_int_or(0),
            value.get("ChunkZ").as_int_or(0),
        );
        let child_list = value.get("Children").as_list(|child| child);
        if let Some(first) = child_list.first()
            && let Some(id) = first
                .get("id")
                .as_string()
                .result()
                .and_then(|id| legacy_to_current(&id))
        {
            value = value.set("id", value.create_string(id));
        }
        if let Some(id) = value.get("id").as_string().result() {
            extracted
                .entry(pos)
                .or_insert_with(LegacyStructureData::new)
                .add_start(id.clone(), value);
            let (x, z) = (ChunkPos::get_x(pos), ChunkPos::get_z(pos));
            // int arithmetic of Java: wraps (and then loops zero times) at the ends of the range
            let range =
                |at: i32| at.wrapping_sub(STRUCTURE_RANGE)..=at.wrapping_add(STRUCTURE_RANGE);
            for neighbor_x in range(x) {
                for neighbor_z in range(z) {
                    extracted
                        .entry(ChunkPos::pack_xz(neighbor_x, neighbor_z))
                        .or_insert_with(LegacyStructureData::new)
                        .add_index(&id, pos);
                }
            }
        }
    }
}

/// The compound `key` of `tag`, put there empty first when it is missing (or not a compound).
fn compound_or_insert<'a>(tag: &'a mut CompoundTag, key: &str) -> &'a mut CompoundTag {
    if tag.get_compound(key).is_none() {
        tag.put(key, CompoundTag::new());
    }
    tag.get_compound_mut(key).expect("a compound was put")
}

/// `storeLegacyStructureDataToChunks`: the chunks, a region at a time, get their starts and references.
fn store_legacy_structure_data_to_chunks(
    structures: &BTreeMap<i64, LegacyStructureData>,
    chunks: &mut ChunkNbt,
    ctx: &mut ContentContext<'_>,
    data_fix_context: &CompoundTag,
) -> Result<(), FileFixError> {
    let mut entries: Vec<(&i64, &LegacyStructureData)> = structures.iter().collect();
    entries.sort_by_key(|(pos, _)| {
        ChunkPos::pack_xz(
            ChunkPos::get_region_x_from_long(**pos),
            ChunkPos::get_region_z_from_long(**pos),
        )
    });
    for (pos, legacy_data) in entries {
        if ctx.progress.is_canceled() {
            return Err(FileFixError::Canceled);
        }
        chunks
            .update_chunk(
                ctx.tree,
                &ChunkPos::unpack(*pos),
                Some(data_fix_context),
                |mut tag| {
                    let level_tag = compound_or_insert(&mut tag, "Level");
                    let structure_tag = compound_or_insert(level_tag, "Structures");
                    let start_tag = compound_or_insert(structure_tag, "Starts");
                    for (id, value) in legacy_data.starts.iter() {
                        start_tag.put(
                            id.clone(),
                            value.cast(&NBT).to_tag().expect("a start is a valid tag"),
                        );
                    }
                    let references_tag = compound_or_insert(structure_tag, "References");
                    for (id, indexes) in legacy_data.indexes.iter() {
                        references_tag.put_long_array(id.clone(), indexes.clone());
                    }
                    tag
                },
            )
            // a failed read or write completes the chunk future of vanilla exceptionally, which `join` throws unchecked
            .map_err(|e| FileFixError::Unchecked(e.to_string()))?;
        ctx.progress.increment_finished_operations_by(1);
    }
    Ok(())
}
