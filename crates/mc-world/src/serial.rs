//! `SerializableChunkData`: the exact chunk NBT vanilla 26.3 reads and writes
//! (`net.minecraft.world.level.chunk.storage.SerializableChunkData`).
//!
//! [`SerializableChunkData::parse`] / [`SerializableChunkData::write`] are the pure NBT <-> data conversions;
//! [`SerializableChunkData::from_proto_chunk`] / [`from_level_chunk`](SerializableChunkData::from_level_chunk)
//! are vanilla's `copyOf`, and [`SerializableChunkData::read`] builds the chunk (`read(level, ...)`) without the
//! pieces that need a live level (POI consistency, light engine queues, entity/block-entity instantiation):
//! those are returned in [`LoadedChunkData`] for the game layer.

use crate::below_zero_retrogen::BelowZeroRetrogen;
use crate::blending_data::{BlendingData, BlendingDataPacked};
use crate::chunk::{ChunkAccess, LevelChunk, LevelHeight, ProtoChunk, StructureData};
use crate::data_layer::DataLayer;
use crate::heightmap::HeightmapType;
use crate::level_storage::{DATA_VERSION_TAG, WORLD_VERSION};
use crate::palette::{PaletteError, PalettedContainerFactory};
use crate::section::LevelChunkSection;
use crate::status::{ChunkStatus, ChunkType};
use crate::ticks::{LevelChunkTicks, PackedTicks, ProtoChunkTicks, SavedTick};
use crate::upgrade_data::UpgradeData;
use indexmap::{IndexMap, IndexSet};
use mc_core::pos::ChunkPos;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::collections::BTreeMap;
use std::fmt;

/// `SerializableChunkData.ChunkReadException`: the chunk NBT is unusable (vanilla treats the chunk as corrupt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkReadException(pub String);

impl fmt::Display for ChunkReadException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ChunkReadException {}

impl From<PaletteError> for ChunkReadException {
    fn from(e: PaletteError) -> Self {
        ChunkReadException(e.0)
    }
}

/// `SerializableChunkData.SectionData`.
#[derive(Clone, Debug)]
pub struct SectionData {
    pub y: i32,
    pub chunk_section: Option<LevelChunkSection>,
    pub block_light: Option<DataLayer>,
    pub sky_light: Option<DataLayer>,
}

#[derive(Clone, Debug)]
pub struct SerializableChunkData {
    pub chunk_pos: ChunkPos,
    pub min_section_y: i32,
    pub last_update_time: i64,
    pub inhabited_time: i64,
    pub chunk_status: ChunkStatus,
    pub blending_data: Option<BlendingDataPacked>,
    pub below_zero_retrogen: Option<BelowZeroRetrogen>,
    pub upgrade_data: UpgradeData,
    /// `EnumMap`: raw packed longs per heightmap type, in ordinal order.
    pub heightmaps: BTreeMap<HeightmapType, Vec<i64>>,
    pub packed_ticks: PackedTicks,
    /// `ShortList[]`, one slot per entry of the `PostProcessing` list.
    pub post_processing_sections: Vec<Option<Vec<i16>>>,
    pub light_correct: bool,
    pub section_data: Vec<SectionData>,
    pub entities: Vec<CompoundTag>,
    pub block_entities: Vec<CompoundTag>,
    /// The `structures` compound (`starts` + `References`) exactly as read/written.
    pub structure_data: CompoundTag,
}

/// What the game layer has to finish after [`SerializableChunkData::read`] (vanilla does these with the live level).
pub struct LoadedChunkData {
    pub chunk: LoadedChunk,
    /// `(sectionY, blockLight, skyLight)` for every section that carries light data (`skyLight` is dropped for
    /// dimensions without sky light, like vanilla's `hasSkyLight` check).
    pub light: Vec<(i32, Option<DataLayer>, Option<DataLayer>)>,
    /// For `LevelChunk` results: the `postLoadChunk` input (entities to load as legacy entities, block entities to
    /// instantiate / keep packed). Empty for proto chunks (those are stored inside the chunk).
    pub post_load_entities: Vec<CompoundTag>,
    pub post_load_block_entities: Vec<CompoundTag>,
}

pub enum LoadedChunk {
    Proto(ProtoChunk),
    Level(LevelChunk),
}

/// Level-dependent inputs of [`SerializableChunkData::read`].
pub struct ReadContext<'a> {
    pub level_height: LevelHeight,
    /// `dimensionType().hasSkyLight()`.
    pub has_sky_light: bool,
    pub factory: &'a PalettedContainerFactory,
    /// `structuresRegistry.getValue(id) != null`.
    pub is_known_structure: &'a dyn Fn(&str) -> bool,
}

impl SerializableChunkData {
    /// `SerializableChunkData.parse(levelHeight, containerFactory, chunkData)`; `Ok(None)` is vanilla's `null`
    /// (no `Status` string).
    pub fn parse(
        level_height: LevelHeight,
        factory: &PalettedContainerFactory,
        chunk_data: &CompoundTag,
    ) -> Result<Option<SerializableChunkData>, ChunkReadException> {
        if chunk_data.get_string("Status").is_none() {
            return Ok(None);
        }
        let chunk_pos = ChunkPos::new(
            chunk_data.get_int_or("xPos", 0),
            chunk_data.get_int_or("zPos", 0),
        );
        let last_update_time = chunk_data.get_long_or("LastUpdate", 0);
        let inhabited_time = chunk_data.get_long_or("InhabitedTime", 0);
        let chunk_status = chunk_data
            .get("Status")
            .and_then(ChunkStatus::decode)
            .unwrap_or(ChunkStatus::Empty);
        let upgrade_data = match chunk_data.get_compound("UpgradeData") {
            Some(tag) => UpgradeData::parse(
                tag,
                level_height.min_section_y(),
                level_height.max_section_y(),
            ),
            None => UpgradeData::empty(),
        };
        let light_correct = chunk_data.get_boolean_or("isLightOn", false);
        let blending_data = chunk_data
            .get("blending_data")
            .and_then(BlendingDataPacked::parse);
        let below_zero_retrogen = chunk_data
            .get("below_zero_retrogen")
            .and_then(BelowZeroRetrogen::parse);
        let mut heightmaps = BTreeMap::new();
        if let Some(tag) = chunk_data.get_compound("Heightmaps") {
            for ty in HeightmapType::ALL {
                if let Some(longs) = tag.get_long_array(ty.serialization_key()) {
                    heightmaps.insert(ty, longs.to_vec());
                }
            }
        }
        let block_ticks = SavedTick::<Block>::filter_tick_list_for_chunk(
            &SavedTick::<Block>::read_list(chunk_data, "block_ticks"),
            chunk_pos,
        );
        let fluid_ticks = SavedTick::<Fluid>::filter_tick_list_for_chunk(
            &SavedTick::<Fluid>::read_list(chunk_data, "fluid_ticks"),
            chunk_pos,
        );
        let packed_ticks = PackedTicks {
            blocks: block_ticks,
            fluids: fluid_ticks,
        };

        let post_process_tags = chunk_data.get_list_or_empty("PostProcessing");
        let post_processing_sections = post_process_tags
            .iter()
            .map(|tag| match tag.as_list() {
                Some(offsets) if !offsets.is_empty() => Some(
                    (0..offsets.len())
                        .map(|i| offsets.get_short_or(i, 0))
                        .collect::<Vec<i16>>(),
                ),
                _ => None,
            })
            .collect();

        let compounds = |key: &str| -> Vec<CompoundTag> {
            chunk_data
                .get_list(key)
                .map(|l| l.compounds().cloned().collect())
                .unwrap_or_default()
        };
        let entities = compounds("entities");
        let block_entities = compounds("block_entities");
        let structure_data = chunk_data.get_compound_or_empty("structures").clone();

        let mut section_data = Vec::new();
        for section_tag in chunk_data.get_list_or_empty("sections") {
            let Some(section_tag) = section_tag.as_compound() else {
                continue;
            };
            let y = i32::from(section_tag.get_byte_or("Y", 0));
            let chunk_section =
                if y >= level_height.min_section_y() && y <= level_height.max_section_y() {
                    let log_problems = |problems: &[String]| {
                        for p in problems {
                            log::error!(
                                "Recoverable errors when loading section [{}, {}, {}]: {}",
                                chunk_pos.x,
                                y,
                                chunk_pos.z,
                                p
                            );
                        }
                    };
                    let blocks = match section_tag.get_compound("block_states") {
                        Some(tag) => {
                            let unpacked =
                                factory.block_states_from_nbt(&Tag::Compound(tag.clone()))?;
                            log_problems(&unpacked.problems);
                            unpacked.container
                        }
                        None => factory.create_for_block_states(),
                    };
                    let biomes = match section_tag.get_compound("biomes") {
                        Some(tag) => {
                            let unpacked = factory.biomes_from_nbt(&Tag::Compound(tag.clone()))?;
                            log_problems(&unpacked.problems);
                            unpacked.container
                        }
                        None => factory.create_for_biomes(),
                    };
                    Some(LevelChunkSection::new(blocks, biomes))
                } else {
                    None
                };
            let layer = |key: &str| -> Result<Option<DataLayer>, ChunkReadException> {
                match section_tag.get_byte_array(key) {
                    Some(bytes) => DataLayer::from_bytes(bytes.iter().map(|&b| b as u8).collect())
                        .map(Some)
                        .map_err(|e| {
                            ChunkReadException(format!(
                                "DataLayer should be 2048 bytes not: {}",
                                e.0
                            ))
                        }),
                    None => Ok(None),
                }
            };
            section_data.push(SectionData {
                y,
                chunk_section,
                block_light: layer("BlockLight")?,
                sky_light: layer("SkyLight")?,
            });
        }

        Ok(Some(SerializableChunkData {
            chunk_pos,
            min_section_y: level_height.min_section_y(),
            last_update_time,
            inhabited_time,
            chunk_status,
            blending_data,
            below_zero_retrogen,
            upgrade_data,
            heightmaps,
            packed_ticks,
            post_processing_sections,
            light_correct,
            section_data,
            entities,
            block_entities,
            structure_data,
        }))
    }

    /// `write()`: the chunk NBT (`DataVersion` = current).
    pub fn write(
        &self,
        factory: &PalettedContainerFactory,
    ) -> Result<CompoundTag, ChunkReadException> {
        let mut tag = CompoundTag::new();
        tag.put_int(DATA_VERSION_TAG, WORLD_VERSION);
        tag.put_int("xPos", self.chunk_pos.x);
        tag.put_int("yPos", self.min_section_y);
        tag.put_int("zPos", self.chunk_pos.z);
        tag.put_long("LastUpdate", self.last_update_time);
        tag.put_long("InhabitedTime", self.inhabited_time);
        tag.put_string("Status", self.chunk_status.name());
        if let Some(blending) = &self.blending_data {
            tag.put(
                "blending_data",
                blending.to_tag().map_err(ChunkReadException)?,
            );
        }
        if let Some(retrogen) = &self.below_zero_retrogen {
            tag.put("below_zero_retrogen", retrogen.to_tag());
        }
        if !self.upgrade_data.is_empty() {
            tag.put("UpgradeData", self.upgrade_data.write());
        }

        let mut section_tags = ListTag::new();
        for section in &self.section_data {
            let mut section_tag = CompoundTag::new();
            if let Some(chunk_section) = &section.chunk_section {
                section_tag.put(
                    "block_states",
                    chunk_section
                        .states()
                        .to_nbt(&crate::palette::BlockStateCodec)?,
                );
                section_tag.put("biomes", chunk_section.biomes().to_nbt(&factory.biomes)?);
            }
            if let Some(light) = &section.block_light {
                section_tag.put_byte_array(
                    "BlockLight",
                    light.data().iter().map(|&b| b as i8).collect(),
                );
            }
            if let Some(light) = &section.sky_light {
                section_tag
                    .put_byte_array("SkyLight", light.data().iter().map(|&b| b as i8).collect());
            }
            if !section_tag.is_empty() {
                section_tag.put_byte("Y", section.y as i8);
                section_tags.push(section_tag);
            }
        }
        tag.put("sections", section_tags);
        if self.light_correct {
            tag.put_boolean("isLightOn", true);
        }

        tag.put(
            "block_entities",
            self.block_entities
                .iter()
                .cloned()
                .map(Tag::Compound)
                .collect::<ListTag>(),
        );
        if self.chunk_status.chunk_type() == ChunkType::ProtoChunk {
            tag.put(
                "entities",
                self.entities
                    .iter()
                    .cloned()
                    .map(Tag::Compound)
                    .collect::<ListTag>(),
            );
        }
        tag.put(
            "block_ticks",
            SavedTick::<Block>::write_list(&self.packed_ticks.blocks),
        );
        tag.put(
            "fluid_ticks",
            SavedTick::<Fluid>::write_list(&self.packed_ticks.fluids),
        );
        tag.put(
            "PostProcessing",
            pack_offsets(&self.post_processing_sections),
        );
        let mut heightmaps_tag = CompoundTag::new();
        for (ty, data) in &self.heightmaps {
            heightmaps_tag.put_long_array(ty.serialization_key(), data.clone());
        }
        tag.put("Heightmaps", heightmaps_tag);
        tag.put("structures", self.structure_data.clone());
        Ok(tag)
    }

    /// `getChunkStatusFromTag`.
    pub fn get_chunk_status_from_tag(tag: Option<&CompoundTag>) -> ChunkStatus {
        tag.and_then(|t| t.get("Status"))
            .and_then(ChunkStatus::decode)
            .unwrap_or(ChunkStatus::Empty)
    }

    /// `copyOf(level, chunk)` for the shared state. `light(section_y)` yields the sky/block layers of the light
    /// engine (`None` = no data; empty layers are dropped like vanilla); sections are visited from
    /// `min_section_y - 1` to `max_section_y + 1` (the light section range).
    fn copy_of(
        access: &ChunkAccess,
        status: ChunkStatus,
        below_zero_retrogen: Option<BelowZeroRetrogen>,
        game_time: i64,
        light: &dyn Fn(i32) -> (Option<DataLayer>, Option<DataLayer>),
        block_entities: Vec<CompoundTag>,
        entities: Vec<CompoundTag>,
        packed_ticks: PackedTicks,
    ) -> SerializableChunkData {
        let mut section_data = Vec::new();
        let h = access.level_height;
        for section_y in (h.min_section_y() - 1)..(h.max_section_y() + 2) {
            let index = h.section_index_from_section_y(section_y);
            let has_section = index >= 0 && (index as usize) < access.sections.len();
            let (source_block, source_sky) = light(section_y);
            let block_light = source_block.filter(|l| !l.is_empty()).map(|l| l.copy());
            let sky_light = source_sky.filter(|l| !l.is_empty()).map(|l| l.copy());
            if has_section || block_light.is_some() || sky_light.is_some() {
                section_data.push(SectionData {
                    y: section_y,
                    chunk_section: has_section.then(|| access.sections[index as usize].clone()),
                    block_light,
                    sky_light,
                });
            }
        }
        let heightmaps = access
            .heightmaps
            .iter()
            .map(|(ty, h)| (*ty, h.get_raw_data().to_vec()))
            .collect();
        SerializableChunkData {
            chunk_pos: access.pos,
            min_section_y: h.min_section_y(),
            last_update_time: game_time,
            inhabited_time: access.inhabited_time(),
            chunk_status: status,
            blending_data: access.blending_data.as_ref().map(BlendingData::pack),
            below_zero_retrogen,
            upgrade_data: access.upgrade_data.clone(),
            heightmaps,
            packed_ticks,
            post_processing_sections: access
                .post_processing
                .iter()
                .map(|p| p.clone().filter(|v| !v.is_empty()))
                .collect(),
            light_correct: access.is_light_correct(),
            section_data,
            entities,
            block_entities,
            structure_data: pack_structure_data(&access.structures),
        }
    }

    /// `copyOf` for a proto chunk. `block_entities` are the tags `getBlockEntityNbtForSaving` yields for the chunk's
    /// block entities (live ones saved by the game layer, pending ones taken from `chunk.access.block_entities`).
    pub fn from_proto_chunk(
        chunk: &ProtoChunk,
        game_time: i64,
        light: &dyn Fn(i32) -> (Option<DataLayer>, Option<DataLayer>),
        block_entities: Vec<CompoundTag>,
    ) -> SerializableChunkData {
        Self::copy_of(
            &chunk.access,
            chunk.get_persisted_status(),
            chunk.get_below_zero_retrogen().cloned(),
            game_time,
            light,
            block_entities,
            if chunk.get_persisted_status().chunk_type() == ChunkType::ProtoChunk {
                chunk.entities.clone()
            } else {
                Vec::new()
            },
            chunk.get_ticks_for_serialization(game_time),
        )
    }

    /// `copyOf` for a full chunk (status `FULL`).
    pub fn from_level_chunk(
        chunk: &LevelChunk,
        game_time: i64,
        light: &dyn Fn(i32) -> (Option<DataLayer>, Option<DataLayer>),
        block_entities: Vec<CompoundTag>,
    ) -> SerializableChunkData {
        Self::copy_of(
            &chunk.access,
            ChunkStatus::Full,
            None,
            game_time,
            light,
            block_entities,
            Vec::new(),
            chunk.get_ticks_for_serialization(game_time),
        )
    }

    /// `read(level, poiManager, regionInfo, pos)` minus the live-level parts (see [`LoadedChunkData`]).
    pub fn read(self, ctx: &ReadContext<'_>, pos: ChunkPos) -> LoadedChunkData {
        if pos != self.chunk_pos {
            log::error!(
                "Chunk file at {pos} is in the wrong location; relocating. (Expected {pos}, got {})",
                self.chunk_pos
            );
        }
        let h = ctx.level_height;
        let mut sections: Vec<Option<LevelChunkSection>> = vec![None; h.sections_count()];
        let mut light = Vec::new();
        for section in self.section_data {
            if let Some(chunk_section) = section.chunk_section {
                let index = h.section_index_from_section_y(section.y);
                if index >= 0 && (index as usize) < sections.len() {
                    sections[index as usize] = Some(chunk_section);
                }
            }
            let sky = if ctx.has_sky_light {
                section.sky_light
            } else {
                None
            };
            if section.block_light.is_some() || sky.is_some() {
                light.push((section.y, section.block_light, sky));
            }
        }

        let chunk_type = self.chunk_status.chunk_type();
        let void_air = chunk_type != ChunkType::LevelChunk;
        let blending = BlendingData::unpack(self.blending_data.as_ref());
        let (loaded, post_load_entities, post_load_block_entities);
        if chunk_type == ChunkType::LevelChunk {
            let mut chunk = LevelChunk::new(
                h,
                ctx.factory,
                pos,
                self.upgrade_data,
                LevelChunkTicks::from_pending(self.packed_ticks.blocks),
                LevelChunkTicks::from_pending(self.packed_ticks.fluids),
                self.inhabited_time,
                Some(sections),
                blending,
            );
            chunk.access.set_light_correct(self.light_correct);
            Self::finish(
                &mut chunk.access,
                ChunkStatus::Full,
                void_air,
                &self.heightmaps,
                &self.structure_data,
                pos,
                ctx,
                &self.post_processing_sections,
            );
            loaded = LoadedChunk::Level(chunk);
            post_load_entities = self.entities;
            post_load_block_entities = self.block_entities;
        } else {
            let mut proto = ProtoChunk::with_sections(
                pos,
                self.upgrade_data,
                Some(sections),
                ProtoChunkTicks::load(&self.packed_ticks.blocks),
                ProtoChunkTicks::load(&self.packed_ticks.fluids),
                h,
                ctx.factory,
                blending,
            );
            proto.access.set_inhabited_time(self.inhabited_time);
            if let Some(retrogen) = self.below_zero_retrogen {
                proto.set_below_zero_retrogen(Some(retrogen));
            }
            proto.set_persisted_status(self.chunk_status);
            if self.chunk_status.is_or_after(ChunkStatus::InitializeLight) {
                proto.set_light_engine_attached(true);
            }
            proto.access.set_light_correct(self.light_correct);
            let status = proto.get_persisted_status();
            Self::finish(
                &mut proto.access,
                status,
                void_air,
                &self.heightmaps,
                &self.structure_data,
                pos,
                ctx,
                &self.post_processing_sections,
            );
            for entity in self.entities {
                proto.add_entity(entity);
            }
            for block_entity in self.block_entities {
                proto.access.set_block_entity_nbt(block_entity);
            }
            loaded = LoadedChunk::Proto(proto);
            post_load_entities = Vec::new();
            post_load_block_entities = Vec::new();
        }
        LoadedChunkData {
            chunk: loaded,
            light,
            post_load_entities,
            post_load_block_entities,
        }
    }

    /// Heightmaps, structures and post-processing (`read` after the chunk object exists).
    fn finish(
        access: &mut ChunkAccess,
        persisted_status: ChunkStatus,
        void_air: bool,
        heightmaps: &BTreeMap<HeightmapType, Vec<i64>>,
        structure_data: &CompoundTag,
        pos: ChunkPos,
        ctx: &ReadContext<'_>,
        post_processing: &[Option<Vec<i16>>],
    ) {
        let mut to_prime: Vec<HeightmapType> = persisted_status.heightmaps_after().to_vec();
        for (ty, data) in heightmaps {
            access.set_heightmap(void_air, *ty, data);
            to_prime.retain(|t| t != ty);
        }
        access.prime_heightmaps(void_air, &to_prime);
        access.set_all_starts(unpack_structure_starts(
            structure_data,
            ctx.is_known_structure,
        ));
        access.set_all_references(unpack_structure_references(
            structure_data,
            pos,
            ctx.is_known_structure,
        ));
        for (index, offsets) in post_processing.iter().enumerate() {
            if let Some(offsets) = offsets
                && index < access.post_processing.len()
            {
                access.add_packed_post_process(offsets, index);
            }
        }
    }
}

/// `packOffsets`.
fn pack_offsets(sections: &[Option<Vec<i16>>]) -> ListTag {
    sections
        .iter()
        .map(|offsets| {
            Tag::List(
                offsets
                    .iter()
                    .flatten()
                    .map(|&o| Tag::Short(o))
                    .collect::<ListTag>(),
            )
        })
        .collect()
}

/// `packStructureData`: `{starts: {id: start}, References: {id: long[]}}` (empty reference sets are skipped).
pub fn pack_structure_data(data: &StructureData) -> CompoundTag {
    let mut starts = CompoundTag::new();
    for (key, start) in &data.starts {
        starts.put(key.clone(), Tag::Compound(start.clone()));
    }
    let mut references = CompoundTag::new();
    for (key, set) in &data.references {
        if !set.is_empty() {
            references.put_long_array(key.clone(), set.iter().copied().collect());
        }
    }
    let mut out = CompoundTag::new();
    out.put("starts", starts);
    out.put("References", references);
    out
}

/// `unpackStructureStart`: unknown structures are logged and dropped, as are starts whose `id` is `INVALID`
/// (`StructureStart.loadStaticStart`). The start NBT itself stays raw.
pub fn unpack_structure_starts(
    tag: &CompoundTag,
    is_known: &dyn Fn(&str) -> bool,
) -> IndexMap<String, CompoundTag> {
    let mut out = IndexMap::new();
    let starts = tag.get_compound_or_empty("starts");
    for (key, value) in starts {
        if !is_known(key) {
            log::error!("Unknown structure start: {key}");
            continue;
        }
        let start = value.as_compound().cloned().unwrap_or_default();
        if start.get_string_or("id", "INVALID") == "INVALID" {
            continue;
        }
        out.insert(key.clone(), start);
    }
    out
}

/// `unpackStructureReferences`: unknown structures and references farther than 8 chunks are dropped with a warning.
pub fn unpack_structure_references(
    tag: &CompoundTag,
    pos: ChunkPos,
    is_known: &dyn Fn(&str) -> bool,
) -> IndexMap<String, IndexSet<i64>> {
    let mut out = IndexMap::new();
    for (key, entry) in tag.get_compound_or_empty("References") {
        if !is_known(key) {
            log::warn!("Found reference to unknown structure '{key}' in chunk {pos}, discarding");
            continue;
        }
        let Some(longs) = entry.as_long_array() else {
            continue;
        };
        let set = longs
            .iter()
            .copied()
            .filter(|&packed| {
                let ref_pos = ChunkPos::unpack(packed);
                if ref_pos.get_chessboard_distance(&pos) > 8 {
                    log::warn!(
                        "Found invalid structure reference [ {key} @ {ref_pos} ] for chunk {pos}."
                    );
                    false
                } else {
                    true
                }
            })
            .collect();
        out.insert(key.clone(), set);
    }
    out
}
