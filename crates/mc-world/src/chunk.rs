//! Chunk data model: `LevelHeightAccessor`, the state shared by `ChunkAccess`, plus the data parts of
//! `ProtoChunk` and `LevelChunk` (`net.minecraft.world.level.chunk.{ChunkAccess, ProtoChunk, LevelChunk}`).
//!
//! Everything that needs a live level (block entity instances, entities, tickers, game events, neighbour
//! updates, the light engine) is the business of `mc-game`/`mc-light`: block entities are kept as the raw
//! `CompoundTag`s vanilla keeps in `pendingBlockEntities`, entities of proto chunks as raw tags, structure
//! starts/references as raw NBT keyed by structure id, and light changes are reported through
//! [`LightEngineHook`].

use crate::below_zero_retrogen::{self, BelowZeroRetrogen};
use crate::blending_data::BlendingData;
use crate::heightmap::{Heightmap, HeightmapChunk, HeightmapMap, HeightmapType};
use crate::palette::{BiomeId, PalettedContainerFactory};
use crate::section::LevelChunkSection;
use crate::sky_light_sources::ChunkSkyLightSources;
use crate::status::ChunkStatus;
use crate::ticks::{LevelChunkTicks, PackedTicks, ProtoChunkTicks, SerializableTickContainer};
use crate::upgrade_data::UpgradeData;
use indexmap::{IndexMap, IndexSet};
use mc_core::java_collections::{JavaHashMap, block_pos_hash, hash_set_union_order};
use mc_core::math::mth;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos, quart_pos};
use mc_data::block::{Block, BlockState};
use mc_data::fluid::{Fluid, FluidState};
use mc_nbt::{CompoundTag, Tag};

/// `LevelHeightAccessor` for a dimension (or `BelowZeroRetrogen.UPGRADE_HEIGHT_ACCESSOR`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LevelHeight {
    pub min_y: i32,
    pub height: i32,
}

impl LevelHeight {
    pub const fn new(min_y: i32, height: i32) -> Self {
        LevelHeight { min_y, height }
    }

    /// `BelowZeroRetrogen.UPGRADE_HEIGHT_ACCESSOR`.
    pub const UPGRADE: LevelHeight = LevelHeight::new(
        below_zero_retrogen::UPGRADE_MIN_Y,
        below_zero_retrogen::UPGRADE_HEIGHT,
    );

    pub const fn max_y(&self) -> i32 {
        self.min_y + self.height - 1
    }

    pub const fn min_section_y(&self) -> i32 {
        SectionPos::block_to_section_coord_i32(self.min_y)
    }

    pub const fn max_section_y(&self) -> i32 {
        SectionPos::block_to_section_coord_i32(self.max_y())
    }

    pub const fn sections_count(&self) -> usize {
        (self.max_section_y() - self.min_section_y() + 1) as usize
    }

    pub const fn is_outside_build_height(&self, y: i32) -> bool {
        y < self.min_y || y > self.max_y()
    }

    pub const fn is_inside_build_height(&self, y: i32) -> bool {
        !self.is_outside_build_height(y)
    }

    /// `getSectionIndex(blockY)` (may be out of `0..sections_count`).
    pub const fn section_index(&self, block_y: i32) -> i32 {
        self.section_index_from_section_y(SectionPos::block_to_section_coord_i32(block_y))
    }

    pub const fn section_index_from_section_y(&self, section_y: i32) -> i32 {
        section_y - self.min_section_y()
    }

    pub const fn section_y_from_section_index(&self, section_index: i32) -> i32 {
        section_index + self.min_section_y()
    }
}

/// Callbacks into the light engine for [`ProtoChunk::set_block_state`] (vanilla calls
/// `lightEngine.updateSectionStatus/checkBlock`; `skyLightSources.update` is done by the chunk itself).
pub trait LightEngineHook {
    /// `LevelLightEngine.updateSectionStatus(pos, sectionEmpty)`.
    fn update_section_status(&mut self, pos: BlockPos, section_empty: bool);
    /// `LevelLightEngine.checkBlock(pos)`.
    fn check_block(&mut self, pos: BlockPos);
}

/// `LightEngine.hasDifferentLightProperties`.
pub fn has_different_light_properties(old_state: BlockState, new_state: BlockState) -> bool {
    old_state != new_state
        && (new_state.get_light_block() != old_state.get_light_block()
            || new_state.get_light_emission() != old_state.get_light_emission()
            || new_state.use_shape_for_light_occlusion()
            || old_state.use_shape_for_light_occlusion())
}

/// `BlockEntity.getPosFromTag(chunkPos, tag)`: positions outside the chunk are relocated into it.
pub fn block_entity_pos_from_tag(base: ChunkPos, tag: &CompoundTag) -> BlockPos {
    let mut x = tag.get_int_or("x", 0);
    let y = tag.get_int_or("y", 0);
    let mut z = tag.get_int_or("z", 0);
    if SectionPos::block_to_section_coord_i32(x) != base.x
        || SectionPos::block_to_section_coord_i32(z) != base.z
    {
        log::warn!(
            "Block entity {} found in a wrong chunk, expected position from chunk {base}",
            Tag::Compound(tag.clone())
        );
        x = base.get_block_x(SectionPos::section_relative(x));
        z = base.get_block_z(SectionPos::section_relative(z));
    }
    BlockPos::new(x, y, z)
}

/// `ProtoChunk.packOffsetCoordinates`.
pub fn pack_offset_coordinates(pos: BlockPos) -> i16 {
    let (dx, dy, dz) = (pos.x() & 15, pos.y() & 15, pos.z() & 15);
    (dx | dy << 4 | dz << 8) as i16
}

/// `ProtoChunk.unpackOffsetCoordinates`.
pub fn unpack_offset_coordinates(packed: i16, section_y: i32, chunk_pos: ChunkPos) -> BlockPos {
    let packed = i32::from(packed);
    BlockPos::new(
        SectionPos::section_to_block_coord_offset(chunk_pos.x, packed & 15),
        SectionPos::section_to_block_coord_offset(section_y, (packed as u16 as i32) >> 4 & 15),
        SectionPos::section_to_block_coord_offset(chunk_pos.z, (packed as u16 as i32) >> 8 & 15),
    )
}

/// Structure starts (raw `StructureStart` NBT) and references (chunk positions) keyed by structure id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructureData {
    pub starts: IndexMap<String, CompoundTag>,
    pub references: IndexMap<String, IndexSet<i64>>,
}

/// Read-only view of the sections for heightmap computation (`HeightmapChunk`).
pub struct SectionsView<'a> {
    pub level_height: LevelHeight,
    pub sections: &'a [LevelChunkSection],
    /// `ProtoChunk` answers `VOID_AIR` outside the build height, `LevelChunk` plain `AIR`.
    pub void_air_outside: bool,
}

impl HeightmapChunk for SectionsView<'_> {
    fn min_y(&self) -> i32 {
        self.level_height.min_y
    }
    fn height(&self) -> i32 {
        self.level_height.height
    }
    fn highest_section_position(&self) -> i32 {
        highest_section_position(&self.level_height, self.sections)
    }
    fn get_block_state(&self, x: i32, y: i32, z: i32) -> BlockState {
        block_state_in(
            &self.level_height,
            self.sections,
            x,
            y,
            z,
            self.void_air_outside,
        )
    }
}

fn highest_filled_section_index(sections: &[LevelChunkSection]) -> i32 {
    sections
        .iter()
        .rposition(|s| !s.has_only_air())
        .map_or(-1, |i| i as i32)
}

fn highest_section_position(h: &LevelHeight, sections: &[LevelChunkSection]) -> i32 {
    match highest_filled_section_index(sections) {
        -1 => h.min_y,
        i => SectionPos::section_to_block_coord(h.section_y_from_section_index(i)),
    }
}

/// `getBlockState` on chunk-local `x`/`z` and absolute `y`.
fn block_state_in(
    h: &LevelHeight,
    sections: &[LevelChunkSection],
    x: i32,
    y: i32,
    z: i32,
    void_air: bool,
) -> BlockState {
    if void_air && h.is_outside_build_height(y) {
        return Block::VOID_AIR.default_state();
    }
    let index = h.section_index(y);
    if index >= 0 && (index as usize) < sections.len() {
        let section = &sections[index as usize];
        if !section.has_only_air() {
            return section.get_block_state(x & 15, y & 15, z & 15);
        }
    }
    Block::AIR.default_state()
}

/// State shared by `ProtoChunk` and `LevelChunk` (vanilla `ChunkAccess`).
#[derive(Clone, Debug)]
pub struct ChunkAccess {
    pub pos: ChunkPos,
    pub level_height: LevelHeight,
    pub sections: Vec<LevelChunkSection>,
    pub heightmaps: HeightmapMap,
    /// `ShortList[] postProcessing`, one (possibly absent) list of packed offsets per section.
    pub post_processing: Vec<Option<Vec<i16>>>,
    pub upgrade_data: UpgradeData,
    pub blending_data: Option<BlendingData>,
    pub structures: StructureData,
    /// `pendingBlockEntities` (raw block entity NBT keyed by position). Live instances are `mc-game`'s.
    pub block_entities: JavaHashMap<BlockPos, CompoundTag>,
    /// `skyLightSources`.
    pub sky_light_sources: ChunkSkyLightSources,
    inhabited_time: i64,
    unsaved: bool,
    light_correct: bool,
}

impl ChunkAccess {
    /// `new ChunkAccess(...)`: missing sections are replaced by empty ones; a wrongly sized array is ignored with a warning.
    pub fn new(
        pos: ChunkPos,
        upgrade_data: UpgradeData,
        level_height: LevelHeight,
        factory: &PalettedContainerFactory,
        inhabited_time: i64,
        sections: Option<Vec<Option<LevelChunkSection>>>,
        blending_data: Option<BlendingData>,
    ) -> Self {
        let count = level_height.sections_count();
        let mut slots: Vec<Option<LevelChunkSection>> = vec![None; count];
        if let Some(given) = sections {
            if given.len() == count {
                slots = given;
            } else {
                log::warn!(
                    "Could not set level chunk sections, array length is {} instead of {}",
                    given.len(),
                    count
                );
            }
        }
        let sections = slots
            .into_iter()
            .map(|s| s.unwrap_or_else(|| LevelChunkSection::empty(factory)))
            .collect();
        ChunkAccess {
            pos,
            level_height,
            sections,
            heightmaps: HeightmapMap::new(),
            post_processing: vec![None; count],
            upgrade_data,
            blending_data,
            structures: StructureData::default(),
            block_entities: JavaHashMap::new(block_pos_hash),
            sky_light_sources: ChunkSkyLightSources::new(level_height.min_y, level_height.height),
            inhabited_time,
            unsaved: false,
            light_correct: false,
        }
    }

    pub fn min_y(&self) -> i32 {
        self.level_height.min_y
    }

    pub fn height(&self) -> i32 {
        self.level_height.height
    }

    pub fn view(&self, void_air_outside: bool) -> SectionsView<'_> {
        SectionsView {
            level_height: self.level_height,
            sections: &self.sections,
            void_air_outside,
        }
    }

    /// `initializeLightSources()`.
    pub fn initialize_light_sources(&mut self) {
        self.sky_light_sources
            .fill_from(self.level_height.min_section_y(), &self.sections);
    }

    pub fn get_highest_filled_section_index(&self) -> i32 {
        highest_filled_section_index(&self.sections)
    }

    pub fn get_highest_section_position(&self) -> i32 {
        highest_section_position(&self.level_height, &self.sections)
    }

    pub fn get_section(&self, section_index: usize) -> &LevelChunkSection {
        &self.sections[section_index]
    }

    pub fn get_section_mut(&mut self, section_index: usize) -> &mut LevelChunkSection {
        &mut self.sections[section_index]
    }

    /// `getBlockEntitiesPos()`: `new HashSet<>(pendingBlockEntities.keySet())` plus the keys of the live block entities
    /// (`live`, in their map's order), in the set's iteration order.
    pub fn get_block_entities_pos(&self, live: impl Iterator<Item = BlockPos>) -> Vec<BlockPos> {
        hash_set_union_order(
            self.block_entities
                .keys()
                .copied()
                .collect::<Vec<_>>()
                .into_iter(),
            live,
            block_pos_hash,
        )
    }

    /// `setBlockEntityNbt(tag)`: `pendingBlockEntities.put` (an existing entry is replaced; live instances do not exist here).
    pub fn set_block_entity_nbt(&mut self, tag: CompoundTag) {
        let pos = block_entity_pos_from_tag(self.pos, &tag);
        self.block_entities.insert(pos, tag);
    }

    pub fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<&CompoundTag> {
        self.block_entities.get(&pos)
    }

    pub fn remove_block_entity(&mut self, pos: BlockPos) {
        self.block_entities.remove(&pos);
    }

    /// `getOrCreateHeightmapUnprimed(type)`.
    pub fn get_or_create_heightmap_unprimed(&mut self, ty: HeightmapType) -> &mut Heightmap {
        let (min_y, height) = (self.level_height.min_y, self.level_height.height);
        self.heightmaps
            .entry(ty)
            .or_insert_with(|| Heightmap::new(min_y, height, ty))
    }

    pub fn has_primed_heightmap(&self, ty: HeightmapType) -> bool {
        self.heightmaps.contains_key(&ty)
    }

    /// `Heightmap.primeHeightmaps(this, types)`.
    pub fn prime_heightmaps(&mut self, void_air_outside: bool, types: &[HeightmapType]) {
        let view = SectionsView {
            level_height: self.level_height,
            sections: &self.sections,
            void_air_outside,
        };
        Heightmap::prime_heightmaps(&view, &mut self.heightmaps, types);
    }

    /// `setHeightmap(type, data)`: a wrongly sized array is ignored with a warning and the heightmap primed instead.
    pub fn set_heightmap(&mut self, void_air_outside: bool, ty: HeightmapType, data: &[i64]) {
        if let Err(mismatch) = self.get_or_create_heightmap_unprimed(ty).set_raw_data(data) {
            log::warn!("{}", mismatch.message(self.pos));
            self.prime_heightmaps(void_air_outside, &[ty]);
        }
    }

    /// `getHeight(type, x, z)`.
    pub fn get_height(&mut self, void_air_outside: bool, ty: HeightmapType, x: i32, z: i32) -> i32 {
        if !self.heightmaps.contains_key(&ty) {
            self.prime_heightmaps(void_air_outside, &[ty]);
        }
        self.heightmaps[&ty].get_first_available(x & 15, z & 15) - 1
    }

    /// `setAllStarts`.
    pub fn set_all_starts(&mut self, starts: IndexMap<String, CompoundTag>) {
        self.structures.starts = starts;
        self.mark_unsaved();
    }

    /// `setAllReferences`.
    pub fn set_all_references(&mut self, references: IndexMap<String, IndexSet<i64>>) {
        self.structures.references = references;
        self.mark_unsaved();
    }

    pub fn set_start_for_structure(&mut self, structure: String, start: CompoundTag) {
        self.structures.starts.insert(structure, start);
        self.mark_unsaved();
    }

    pub fn add_reference_for_structure(&mut self, structure: String, reference: i64) {
        self.structures
            .references
            .entry(structure)
            .or_default()
            .insert(reference);
        self.mark_unsaved();
    }

    /// `isYSpaceEmpty(yStartInclusive, yEndInclusive)`.
    pub fn is_y_space_empty(&self, y_start: i32, y_end: i32) -> bool {
        let y_start = y_start.max(self.min_y());
        let y_end = y_end.min(self.level_height.max_y());
        let mut y = y_start;
        while y <= y_end {
            if !self.sections[self.level_height.section_index(y) as usize].has_only_air() {
                return false;
            }
            y += 16;
        }
        true
    }

    pub fn mark_unsaved(&mut self) {
        self.unsaved = true;
    }

    /// `tryMarkSaved()`.
    pub fn try_mark_saved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }

    pub fn is_unsaved(&self) -> bool {
        self.unsaved
    }

    pub fn inhabited_time(&self) -> i64 {
        self.inhabited_time
    }

    pub fn increment_inhabited_time(&mut self) {
        self.inhabited_time = self.inhabited_time.wrapping_add(1);
    }

    pub fn set_inhabited_time(&mut self, inhabited_time: i64) {
        self.inhabited_time = inhabited_time;
    }

    pub fn is_light_correct(&self) -> bool {
        self.light_correct
    }

    pub fn set_light_correct(&mut self, light_correct: bool) {
        self.light_correct = light_correct;
        self.mark_unsaved();
    }

    /// `addPackedPostProcess(offsets, sectionIndex)`.
    pub fn add_packed_post_process(&mut self, packed_offsets: &[i16], section_index: usize) {
        self.post_processing[section_index]
            .get_or_insert_with(Vec::new)
            .extend_from_slice(packed_offsets);
    }

    /// `isOldNoiseGeneration()`.
    pub fn is_old_noise_generation(&self) -> bool {
        self.blending_data.is_some()
    }

    /// `getNoiseBiome(quartX, quartY, quartZ)` (the `ChunkAccess` implementation).
    pub fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        let quart_min_y = quart_pos::from_block(self.min_y());
        let quart_max_y = quart_min_y + quart_pos::from_block(self.height()) - 1;
        let clamped = mth::clamp_i32(quart_y, quart_min_y, quart_max_y);
        let index = self
            .level_height
            .section_index(quart_pos::to_block(clamped));
        self.sections[index as usize].get_noise_biome(quart_x & 3, clamped & 3, quart_z & 3)
    }

    /// `fillBiomesFromNoise(resolver)` over `height_for_generation`'s sections.
    pub fn fill_biomes_from_noise(
        &mut self,
        height_for_generation: LevelHeight,
        mut resolver: impl FnMut(i32, i32, i32) -> BiomeId,
    ) {
        let quart_min_x = quart_pos::from_block(self.pos.get_min_block_x());
        let quart_min_z = quart_pos::from_block(self.pos.get_min_block_z());
        for section_y in
            height_for_generation.min_section_y()..=height_for_generation.max_section_y()
        {
            let index = self.level_height.section_index_from_section_y(section_y) as usize;
            self.sections[index].fill_biomes_from_noise(
                &mut resolver,
                quart_min_x,
                quart_pos::from_section(section_y),
                quart_min_z,
            );
        }
    }

    /// [`Self::fill_biomes_from_noise`] with a resolver that also gets the biome currently stored at the cell.
    pub fn fill_biomes_from_noise_with_current(
        &mut self,
        height_for_generation: LevelHeight,
        mut resolver: impl FnMut(i32, i32, i32, BiomeId) -> BiomeId,
    ) {
        let quart_min_x = quart_pos::from_block(self.pos.get_min_block_x());
        let quart_min_z = quart_pos::from_block(self.pos.get_min_block_z());
        for section_y in
            height_for_generation.min_section_y()..=height_for_generation.max_section_y()
        {
            let index = self.level_height.section_index_from_section_y(section_y) as usize;
            self.sections[index].fill_biomes_from_noise_with_current(
                &mut resolver,
                quart_min_x,
                quart_pos::from_section(section_y),
                quart_min_z,
            );
        }
    }

    /// `collectBiomesInPalette(output)`.
    pub fn collect_biomes_in_palette(&self, mut output: impl FnMut(BiomeId)) {
        for section in &self.sections {
            section.biomes().for_each_in_palette(&mut output);
        }
    }

    /// `findBlocks(predicate, consumer)` over every section that may hold a matching state; `consumer` gets the
    /// absolute position in vanilla's iteration order (section by section, then x, y, z of `BlockScanUtils`).
    pub fn find_blocks(
        &self,
        mut predicate: impl FnMut(BlockState) -> bool,
        mut consumer: impl FnMut(BlockPos, BlockState),
    ) {
        for section_y in self.level_height.min_section_y()..=self.level_height.max_section_y() {
            let section =
                &self.sections[self.level_height.section_index_from_section_y(section_y) as usize];
            if section.maybe_has(&mut predicate) {
                let origin = SectionPos::of_chunk(&self.pos, section_y).origin();
                for y in 0..16 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let state = section.get_block_state(x, y, z);
                            if predicate(state) {
                                consumer(origin.offset(x, y, z), state);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------ ProtoChunk

#[derive(Clone, Debug)]
pub struct ProtoChunk {
    pub access: ChunkAccess,
    status: ChunkStatus,
    pub entities: Vec<CompoundTag>,
    below_zero_retrogen: Option<BelowZeroRetrogen>,
    pub block_ticks: ProtoChunkTicks<Block>,
    pub fluid_ticks: ProtoChunkTicks<Fluid>,
    light_engine_attached: bool,
}

impl ProtoChunk {
    /// `new ProtoChunk(pos, upgradeData, levelHeight, factory, blendingData)`.
    pub fn new(
        pos: ChunkPos,
        upgrade_data: UpgradeData,
        level_height: LevelHeight,
        factory: &PalettedContainerFactory,
        blending_data: Option<BlendingData>,
    ) -> Self {
        Self::with_sections(
            pos,
            upgrade_data,
            None,
            ProtoChunkTicks::new(),
            ProtoChunkTicks::new(),
            level_height,
            factory,
            blending_data,
        )
    }

    pub fn with_sections(
        pos: ChunkPos,
        upgrade_data: UpgradeData,
        sections: Option<Vec<Option<LevelChunkSection>>>,
        block_ticks: ProtoChunkTicks<Block>,
        fluid_ticks: ProtoChunkTicks<Fluid>,
        level_height: LevelHeight,
        factory: &PalettedContainerFactory,
        blending_data: Option<BlendingData>,
    ) -> Self {
        ProtoChunk {
            access: ChunkAccess::new(
                pos,
                upgrade_data,
                level_height,
                factory,
                0,
                sections,
                blending_data,
            ),
            status: ChunkStatus::Empty,
            entities: Vec::new(),
            below_zero_retrogen: None,
            block_ticks,
            fluid_ticks,
            light_engine_attached: false,
        }
    }

    pub fn pos(&self) -> ChunkPos {
        self.access.pos
    }

    pub fn get_persisted_status(&self) -> ChunkStatus {
        self.status
    }

    /// `setPersistedStatus(status)`.
    pub fn set_persisted_status(&mut self, status: ChunkStatus) {
        self.status = status;
        if let Some(retrogen) = &self.below_zero_retrogen
            && status.is_or_after(retrogen.target_status())
        {
            self.below_zero_retrogen = None;
        }
        self.access.mark_unsaved();
    }

    pub fn get_below_zero_retrogen(&self) -> Option<&BelowZeroRetrogen> {
        self.below_zero_retrogen.as_ref()
    }

    pub fn set_below_zero_retrogen(&mut self, retrogen: Option<BelowZeroRetrogen>) {
        self.below_zero_retrogen = retrogen;
    }

    /// `getHighestGeneratedStatus()`.
    pub fn get_highest_generated_status(&self) -> ChunkStatus {
        match &self.below_zero_retrogen {
            Some(r) => ChunkStatus::max(r.target_status(), self.status),
            None => self.status,
        }
    }

    /// `isUpgrading()`.
    pub fn is_upgrading(&self) -> bool {
        self.below_zero_retrogen.is_some()
    }

    /// `getHeightAccessorForGeneration()`.
    pub fn get_height_accessor_for_generation(&self) -> LevelHeight {
        if self.is_upgrading() {
            LevelHeight::UPGRADE
        } else {
            self.access.level_height
        }
    }

    /// `setLightEngine(lightEngine)`: from now on [`ProtoChunk::set_block_state`] reports to the hook it is given.
    pub fn set_light_engine_attached(&mut self, attached: bool) {
        self.light_engine_attached = attached;
    }

    /// `getNoiseBiome`: only valid once biomes exist.
    pub fn get_noise_biome(
        &self,
        quart_x: i32,
        quart_y: i32,
        quart_z: i32,
    ) -> Result<BiomeId, &'static str> {
        if self
            .get_highest_generated_status()
            .is_or_after(ChunkStatus::Biomes)
        {
            Ok(self.access.get_noise_biome(quart_x, quart_y, quart_z))
        } else {
            Err("Asking for biomes before we have biomes")
        }
    }

    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        let y = pos.y();
        if self.access.level_height.is_outside_build_height(y) {
            return Block::VOID_AIR.default_state();
        }
        let section = &self.access.sections[self.access.level_height.section_index(y) as usize];
        if section.has_only_air() {
            Block::AIR.default_state()
        } else {
            section.get_block_state(pos.x() & 15, y & 15, pos.z() & 15)
        }
    }

    pub fn get_fluid_state(&self, pos: BlockPos) -> FluidState {
        let y = pos.y();
        if self.access.level_height.is_outside_build_height(y) {
            return FluidState::EMPTY;
        }
        let section = &self.access.sections[self.access.level_height.section_index(y) as usize];
        if section.has_only_air() {
            FluidState::EMPTY
        } else {
            section.get_fluid_state(pos.x() & 15, y & 15, pos.z() & 15)
        }
    }

    /// `setBlockState(pos, state, flags)` (flags are irrelevant for proto chunks): returns the previous state
    /// (`VOID_AIR` outside the build height, `state` itself when it is air into an empty section).
    pub fn set_block_state(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        light: Option<&mut dyn LightEngineHook>,
    ) -> BlockState {
        let (x, y, z) = (pos.x(), pos.y(), pos.z());
        if self.access.level_height.is_outside_build_height(y) {
            return Block::VOID_AIR.default_state();
        }
        let section_index = self.access.level_height.section_index(y) as usize;
        let was_empty = self.access.sections[section_index].has_only_air();
        if was_empty && state.block() == Block::AIR {
            return state;
        }
        let (local_x, local_y, local_z) = (
            SectionPos::section_relative(x),
            SectionPos::section_relative(y),
            SectionPos::section_relative(z),
        );
        let section = &mut self.access.sections[section_index];
        let old_state = section.set_block_state(local_x, local_y, local_z, state);
        if self.status.is_or_after(ChunkStatus::InitializeLight) && self.light_engine_attached {
            let is_empty = section.has_only_air();
            let mut light = light;
            if is_empty != was_empty
                && let Some(light) = light.as_deref_mut()
            {
                light.update_section_status(pos, is_empty);
            }
            if has_different_light_properties(old_state, state) {
                let view = SectionsView {
                    level_height: self.access.level_height,
                    sections: &self.access.sections,
                    void_air_outside: true,
                };
                self.access
                    .sky_light_sources
                    .update(&view, local_x, y, local_z);
                if let Some(light) = light {
                    light.check_block(pos);
                }
            }
        }

        let after = self.status.heightmaps_after();
        let mut to_prime: Vec<HeightmapType> = Vec::new();
        let view = SectionsView {
            level_height: self.access.level_height,
            sections: &self.access.sections,
            void_air_outside: true,
        };
        for &ty in after {
            match self.access.heightmaps.get_mut(&ty) {
                None => to_prime.push(ty),
                Some(heightmap) => {
                    heightmap.update(local_x, y, local_z, state, &view);
                }
            }
        }
        if !to_prime.is_empty() {
            Heightmap::prime_heightmaps(&view, &mut self.access.heightmaps, &to_prime);
        }
        old_state
    }

    /// `markPosForPostProcessing(pos)`.
    pub fn mark_pos_for_post_processing(&mut self, pos: BlockPos) {
        if self.access.level_height.is_inside_build_height(pos.y()) {
            let index = self.access.level_height.section_index(pos.y()) as usize;
            self.access.post_processing[index]
                .get_or_insert_with(Vec::new)
                .push(pack_offset_coordinates(pos));
        }
    }

    /// `addEntity(tag)`.
    pub fn add_entity(&mut self, tag: CompoundTag) {
        self.entities.push(tag);
    }

    /// `setStartForStructure`: with a below-zero retrogen, a valid start reaching outside the generation height is dropped.
    /// `bounding_y` is `(minY, maxY)` of the start's bounding box when the start is valid.
    pub fn set_start_for_structure(
        &mut self,
        structure: String,
        start: CompoundTag,
        bounding_y: Option<(i32, i32)>,
    ) {
        if self.below_zero_retrogen.is_some()
            && let Some((min, max)) = bounding_y
        {
            let h = self.get_height_accessor_for_generation();
            if min < h.min_y || max > h.max_y() {
                return;
            }
        }
        self.access.set_start_for_structure(structure, start);
    }

    /// `getTicksForSerialization(currentTick)`.
    pub fn get_ticks_for_serialization(&self, current_tick: i64) -> PackedTicks {
        PackedTicks {
            blocks: self.block_ticks.pack(current_tick),
            fluids: self.fluid_ticks.pack(current_tick),
        }
    }
}

// ------------------------------------------------------------------------------------ LevelChunk

/// Raw result of [`LevelChunk::set_block_state_raw`] for the gameplay layer to continue with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawSetBlock {
    pub old_state: BlockState,
    pub section_was_empty: bool,
    pub section_is_empty: bool,
    /// `LightEngine.hasDifferentLightProperties(old, new)`.
    pub light_properties_changed: bool,
}

pub struct LevelChunk {
    pub access: ChunkAccess,
    pub block_ticks: LevelChunkTicks<Block>,
    pub fluid_ticks: LevelChunkTicks<Fluid>,
    loaded: bool,
}

impl LevelChunk {
    /// `new LevelChunk(level, pos, upgradeData, blockTicks, fluidTicks, inhabitedTime, sections, postLoad, blendingData)`.
    pub fn new(
        level_height: LevelHeight,
        factory: &PalettedContainerFactory,
        pos: ChunkPos,
        upgrade_data: UpgradeData,
        block_ticks: LevelChunkTicks<Block>,
        fluid_ticks: LevelChunkTicks<Fluid>,
        inhabited_time: i64,
        sections: Option<Vec<Option<LevelChunkSection>>>,
        blending_data: Option<BlendingData>,
    ) -> Self {
        let mut access = ChunkAccess::new(
            pos,
            upgrade_data,
            level_height,
            factory,
            inhabited_time,
            sections,
            blending_data,
        );
        for ty in HeightmapType::ALL {
            if ChunkStatus::Full.heightmaps_after().contains(&ty) {
                access.heightmaps.insert(
                    ty,
                    Heightmap::new(level_height.min_y, level_height.height, ty),
                );
            }
        }
        LevelChunk {
            access,
            block_ticks,
            fluid_ticks,
            loaded: false,
        }
    }

    /// `new LevelChunk(level, pos)`: an empty chunk.
    pub fn empty(
        level_height: LevelHeight,
        factory: &PalettedContainerFactory,
        pos: ChunkPos,
    ) -> Self {
        Self::new(
            level_height,
            factory,
            pos,
            UpgradeData::empty(),
            LevelChunkTicks::new(),
            LevelChunkTicks::new(),
            0,
            None,
            None,
        )
    }

    /// `new LevelChunk(level, protoChunk, postLoad)` for the data parts (block entity instances, post-load
    /// processing and sky light sources are the gameplay layer's).
    pub fn from_proto(proto: ProtoChunk, factory: &PalettedContainerFactory) -> Self {
        let ProtoChunk {
            access: p,
            block_ticks,
            fluid_ticks,
            ..
        } = proto;
        let light_correct = p.is_light_correct();
        let mut chunk = LevelChunk::new(
            p.level_height,
            factory,
            p.pos,
            p.upgrade_data.clone(),
            LevelChunkTicks::from_pending(block_ticks.scheduled_ticks()),
            LevelChunkTicks::from_pending(fluid_ticks.scheduled_ticks()),
            p.inhabited_time(),
            Some(p.sections.into_iter().map(Some).collect()),
            p.blending_data.clone(),
        );
        if chunk
            .access
            .block_entities
            .keys()
            .any(|k| p.block_entities.contains_key(k))
        {
            log::error!("Chunk at {} contains duplicated block entities", p.pos);
        }
        chunk
            .access
            .block_entities
            .put_all(p.block_entities.into_entries().into_iter());
        chunk.access.post_processing = p.post_processing;
        chunk.access.set_all_starts(p.structures.starts);
        chunk.access.set_all_references(p.structures.references);
        for (ty, heightmap) in &p.heightmaps {
            if ChunkStatus::Full.heightmaps_after().contains(ty) {
                chunk
                    .access
                    .set_heightmap(false, *ty, heightmap.get_raw_data());
            }
        }
        chunk.access.sky_light_sources = p.sky_light_sources;
        chunk.access.set_light_correct(light_correct);
        chunk.access.mark_unsaved();
        chunk
    }

    pub fn pos(&self) -> ChunkPos {
        self.access.pos
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn set_loaded(&mut self, loaded: bool) {
        self.loaded = loaded;
    }

    /// `getBlockState(pos)` (non-debug level).
    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        block_state_in(
            &self.access.level_height,
            &self.access.sections,
            pos.x(),
            pos.y(),
            pos.z(),
            false,
        )
    }

    /// `getFluidState(x, y, z)`.
    pub fn get_fluid_state(&self, x: i32, y: i32, z: i32) -> FluidState {
        let h = &self.access.level_height;
        let index = h.section_index(y);
        if index >= 0 && (index as usize) < self.access.sections.len() {
            let section = &self.access.sections[index as usize];
            if !section.has_only_air() {
                return section.get_fluid_state(x & 15, y & 15, z & 15);
            }
        }
        FluidState::EMPTY
    }

    /// The data part of `setBlockState`: section update plus the four live heightmaps. Returns `None` when vanilla
    /// returns `null` before side effects (air into an empty section, or no change).
    pub fn set_block_state_raw(&mut self, pos: BlockPos, state: BlockState) -> Option<RawSetBlock> {
        let y = pos.y();
        let index = self.access.level_height.section_index(y) as usize;
        let was_empty = self.access.sections[index].has_only_air();
        if was_empty && state.is_air() {
            return None;
        }
        let (local_x, local_y, local_z) = (pos.x() & 15, y & 15, pos.z() & 15);
        let old_state =
            self.access.sections[index].set_block_state(local_x, local_y, local_z, state);
        if old_state == state {
            return None;
        }
        let view = SectionsView {
            level_height: self.access.level_height,
            sections: &self.access.sections,
            void_air_outside: false,
        };
        for ty in [
            HeightmapType::MotionBlocking,
            HeightmapType::MotionBlockingNoLeaves,
            HeightmapType::OceanFloor,
            HeightmapType::WorldSurface,
        ] {
            if let Some(h) = self.access.heightmaps.get_mut(&ty) {
                h.update(local_x, y, local_z, state, &view);
            }
        }
        let light_properties_changed = has_different_light_properties(old_state, state);
        if light_properties_changed {
            self.access
                .sky_light_sources
                .update(&view, local_x, y, local_z);
        }
        Some(RawSetBlock {
            old_state,
            section_was_empty: was_empty,
            section_is_empty: self.access.sections[index].has_only_air(),
            light_properties_changed,
        })
    }

    /// `getTicksForSerialization(currentTick)`.
    pub fn get_ticks_for_serialization(&self, current_tick: i64) -> PackedTicks {
        PackedTicks {
            blocks: self.block_ticks.pack(current_tick),
            fluids: self.fluid_ticks.pack(current_tick),
        }
    }
}
