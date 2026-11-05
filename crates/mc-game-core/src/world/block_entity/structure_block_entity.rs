//! `StructureBlockEntity`: the structure block's name/offset/size/mode settings and the save / load / corner / detect-size
//! operations on a level.
//!
//! Vanilla's methods that read `this.level` are associated functions over `(level, pos)` here, so the entity stays in its
//! chunk while the level is mutated (placing a structure or switching the mode changes blocks around/at the entity).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::structure::templatesystem::block_rot_processor::BlockRotProcessor;
use crate::world::level::levelgen::structure::templatesystem::structure_place_settings::SharedRandom;
use crate::world::level::levelgen::structure::templatesystem::{StructurePlaceSettings, StructureTemplate};
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::{self, RandomSource};
use mc_core::rotation::Rotation;
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_data::enums::StructureMode;
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use std::sync::Arc;

/// `StructureBlockEntity.MAX_OFFSET_PER_AXIS` / `MAX_SIZE_PER_AXIS`.
pub const MAX_OFFSET_PER_AXIS: i32 = 48;
pub const MAX_SIZE_PER_AXIS: i32 = 48;
/// `SCAN_CORNER_BLOCKS_RANGE`.
const SCAN_CORNER_BLOCKS_RANGE: i32 = 80;

pub struct StructureBlockEntity {
    base: BlockEntityData,
    structure_name: Option<Identifier>,
    author: String,
    meta_data: String,
    structure_pos: BlockPos,
    structure_size: Vec3i,
    mirror: Mirror,
    rotation: Rotation,
    mode: StructureMode,
    ignore_entities: bool,
    strict: bool,
    powered: bool,
    show_air: bool,
    show_bounding_box: bool,
    integrity: f32,
    seed: i64,
}

/// `Rotation.LEGACY_CODEC` / `Mirror.LEGACY_CODEC` / `StructureMode.LEGACY_CODEC` names (`Enum.name()`).
fn rotation_name(rotation: Rotation) -> &'static str {
    match rotation {
        Rotation::None => "NONE",
        Rotation::Clockwise90 => "CLOCKWISE_90",
        Rotation::Clockwise180 => "CLOCKWISE_180",
        Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
    }
}

fn rotation_by_name(name: &str) -> Option<Rotation> {
    Rotation::ALL.into_iter().find(|r| rotation_name(*r) == name)
}

fn mirror_name(mirror: Mirror) -> &'static str {
    match mirror {
        Mirror::None => "NONE",
        Mirror::LeftRight => "LEFT_RIGHT",
        Mirror::FrontBack => "FRONT_BACK",
    }
}

fn mirror_by_name(name: &str) -> Option<Mirror> {
    [Mirror::None, Mirror::LeftRight, Mirror::FrontBack].into_iter().find(|m| mirror_name(*m) == name)
}

fn mode_name(mode: StructureMode) -> &'static str {
    match mode {
        StructureMode::Save => "SAVE",
        StructureMode::Load => "LOAD",
        StructureMode::Corner => "CORNER",
        StructureMode::Data => "DATA",
    }
}

fn mode_by_name(name: &str) -> Option<StructureMode> {
    StructureMode::ALL.iter().copied().find(|m| mode_name(*m) == name)
}

/// `createRandom(seed)`.
pub fn create_random(seed: i64) -> Box<dyn RandomSource> {
    if seed == 0 {
        // `Util.getMillis()`
        Box::new(random::create(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)))
    } else {
        Box::new(random::create(seed))
    }
}

impl StructureBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::STRUCTURE_BLOCK, pos, state),
            structure_name: None,
            author: String::new(),
            meta_data: String::new(),
            structure_pos: BlockPos::new(0, 1, 0),
            structure_size: Vec3i::new(0, 0, 0),
            mirror: Mirror::None,
            rotation: Rotation::None,
            mode: state.get(props::STRUCTUREBLOCK_MODE),
            ignore_entities: true,
            strict: false,
            powered: false,
            show_air: false,
            show_bounding_box: true,
            integrity: 1.0,
            seed: 0,
        }
    }

    /// `getStructureName()`.
    pub fn get_structure_name(&self) -> String {
        self.structure_name.as_ref().map_or_else(String::new, ToString::to_string)
    }

    /// `hasStructureName()`.
    pub fn has_structure_name(&self) -> bool {
        self.structure_name.is_some()
    }

    /// `setStructureName(String)`: empty or unparsable names clear the name.
    pub fn set_structure_name(&mut self, name: &str) {
        self.structure_name = if name.is_empty() { None } else { Identifier::try_parse(name) };
    }

    pub fn set_structure_identifier(&mut self, name: Option<Identifier>) {
        self.structure_name = name;
    }

    /// `createdBy(creator)`.
    pub fn created_by(&mut self, creator_plain_text_name: &str) {
        self.author = creator_plain_text_name.to_owned();
    }

    pub fn get_structure_pos(&self) -> BlockPos {
        self.structure_pos
    }

    pub fn set_structure_pos(&mut self, pos: BlockPos) {
        self.structure_pos = pos;
    }

    pub fn get_structure_size(&self) -> Vec3i {
        self.structure_size
    }

    pub fn set_structure_size(&mut self, size: Vec3i) {
        self.structure_size = size;
    }

    pub fn get_mirror(&self) -> Mirror {
        self.mirror
    }

    pub fn set_mirror(&mut self, mirror: Mirror) {
        self.mirror = mirror;
    }

    pub fn get_rotation(&self) -> Rotation {
        self.rotation
    }

    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
    }

    pub fn get_meta_data(&self) -> &str {
        &self.meta_data
    }

    pub fn set_meta_data(&mut self, meta_data: &str) {
        self.meta_data = meta_data.to_owned();
    }

    pub fn get_mode(&self) -> StructureMode {
        self.mode
    }

    pub fn is_ignore_entities(&self) -> bool {
        self.ignore_entities
    }

    pub fn set_ignore_entities(&mut self, ignore_entities: bool) {
        self.ignore_entities = ignore_entities;
    }

    pub fn is_strict(&self) -> bool {
        self.strict
    }

    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    pub fn get_integrity(&self) -> f32 {
        self.integrity
    }

    pub fn set_integrity(&mut self, integrity: f32) {
        self.integrity = integrity;
    }

    pub fn get_seed(&self) -> i64 {
        self.seed
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = seed;
    }

    pub fn is_powered(&self) -> bool {
        self.powered
    }

    pub fn set_powered(&mut self, powered: bool) {
        self.powered = powered;
    }

    pub fn get_show_air(&self) -> bool {
        self.show_air
    }

    pub fn set_show_air(&mut self, show_air: bool) {
        self.show_air = show_air;
    }

    pub fn get_show_bounding_box(&self) -> bool {
        self.show_bounding_box
    }

    pub fn set_show_bounding_box(&mut self, show_bounding_box: bool) {
        self.show_bounding_box = show_bounding_box;
    }

    /// `updateBlockState()` for an entity attached to `level`: the state to set (flags 2) once the entity is in its chunk.
    fn block_state_for_mode(&self, level: &ServerLevel) -> Option<BlockState> {
        let state = level.get_block_state(self.get_block_pos());
        (state.block() == Block::STRUCTURE_BLOCK).then(|| state.set(props::STRUCTUREBLOCK_MODE, self.mode))
    }

    /// `setMode(mode)`: the entity's mode, then the block state follows (`setBlock(pos, state, 2)`).
    pub fn set_mode(level: &mut ServerLevel, pos: BlockPos, mode: StructureMode) {
        if level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.mode = mode).is_none() {
            return;
        }
        let state = level.get_block_state(pos);
        if state.block() == Block::STRUCTURE_BLOCK {
            level.set_block(pos, state.set(props::STRUCTUREBLOCK_MODE, mode), 2, 512);
        }
    }

    /// `usedBy(player)` (server side: only the permission matters, the screen is opened by the client).
    pub fn used_by(can_use_game_master_blocks: bool) -> bool {
        can_use_game_master_blocks
    }

    /// `detectSize()`.
    pub fn detect_size(level: &mut ServerLevel, pos: BlockPos) -> bool {
        let Some((mode, structure_name)) = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| (be.mode, be.structure_name.clone())) else {
            return false;
        };
        if mode != StructureMode::Save {
            return false;
        }
        let Some(bb) = Self::get_enclosing_bounding_box(level, pos, &structure_name) else { return false };
        let delta_x = bb.max_x() - bb.min_x();
        let delta_y = bb.max_y() - bb.min_y();
        let delta_z = bb.max_z() - bb.min_z();
        if delta_x > 1 && delta_y > 1 && delta_z > 1 {
            let state = level.get_block_state(pos);
            level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |level, be| {
                be.structure_pos = BlockPos::new(bb.min_x() - pos.x() + 1, bb.min_y() - pos.y() + 1, bb.min_z() - pos.z() + 1);
                be.structure_size = Vec3i::new(delta_x - 1, delta_y - 1, delta_z - 1);
                level.block_entity_set_changed(pos, be.get_block_state());
            });
            level.send_block_updated(pos, state, state, 3);
            true
        } else {
            false
        }
    }

    /// `getEnclosingBoundingBox(level, selfPos, structureName)`.
    fn get_enclosing_bounding_box(level: &mut ServerLevel, self_pos: BlockPos, structure_name: &Option<Identifier>) -> Option<BoundingBox> {
        let mut corner_count = 0;
        let mut result: Option<BoundingBox> = None;
        let (min_x, max_x) = (self_pos.x() - SCAN_CORNER_BLOCKS_RANGE, self_pos.x() + SCAN_CORNER_BLOCKS_RANGE);
        let (min_z, max_z) = (self_pos.z() - SCAN_CORNER_BLOCKS_RANGE, self_pos.z() + SCAN_CORNER_BLOCKS_RANGE);
        let (min_y, max_y) = (level.get_min_y(), level.get_max_y());
        let min_section_y = level.get_min_y() >> 4;
        // `BlockScanUtils.sectionBasedScan`: chunks (loaded on demand) x, z, then sections by y
        for chunk_x in (min_x >> 4)..=(max_x >> 4) {
            for chunk_z in (min_z >> 4)..=(max_z >> 4) {
                level.chunk_blocking_mut(chunk_x, chunk_z);
                for section_y in (min_y >> 4).max(min_section_y)..=(max_y >> 4).min((level.get_max_y() >> 4)) {
                    let has = level.chunk_now(chunk_x, chunk_z).is_some_and(|chunk| {
                        chunk.data.access.get_section((section_y - min_section_y) as usize).maybe_has(|state| state.block() == Block::STRUCTURE_BLOCK)
                    });
                    if !has {
                        continue;
                    }
                    for y in (section_y << 4).max(min_y)..=((section_y << 4) + 15).min(max_y) {
                        for z in (chunk_z << 4).max(min_z)..=((chunk_z << 4) + 15).min(max_z) {
                            for x in (chunk_x << 4).max(min_x)..=((chunk_x << 4) + 15).min(max_x) {
                                let pos = BlockPos::new(x, y, z);
                                if level.get_block_state(pos).block() != Block::STRUCTURE_BLOCK {
                                    continue;
                                }
                                let Some(be) = level.get_block_entity_immediate(pos) else { continue };
                                let Some(be) = be.as_any().downcast_ref::<StructureBlockEntity>() else { continue };
                                if be.mode == StructureMode::Corner && be.structure_name == *structure_name {
                                    corner_count += 1;
                                    match &mut result {
                                        None => result = Some(BoundingBox::from_block_pos(&pos)),
                                        Some(bb) => {
                                            bb.encapsulate_pos(&pos);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        match corner_count {
            0 => None,
            1 => result.map(|mut bb| {
                bb.encapsulate_pos(&self_pos);
                bb
            }),
            _ => result,
        }
    }

    /// `saveStructure()`: only in `SAVE` mode.
    pub fn save_structure_in_save_mode(level: &mut ServerLevel, pos: BlockPos) -> bool {
        match level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.mode) {
            Some(StructureMode::Save) => Self::save_structure_at(level, pos, true),
            _ => false,
        }
    }

    /// `saveStructure(saveToDisk)` of the structure block at `pos`.
    pub fn save_structure_at(level: &mut ServerLevel, pos: BlockPos, save_to_disk: bool) -> bool {
        let Some((name, structure_pos, size, ignore_entities, author)) = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| {
            (be.structure_name.clone(), be.get_block_pos().offset_vec(&Vec3i::new(be.structure_pos.x(), be.structure_pos.y(), be.structure_pos.z())), be.structure_size, be.ignore_entities, be.author.clone())
        }) else {
            return false;
        };
        match name {
            Some(name) => Self::save_structure(level, &name, structure_pos, size, ignore_entities, &author, save_to_disk, &[]),
            None => false,
        }
    }

    /// `saveStructure(level, structureName, pos, structureSize, ignoreEntities, author, saveToDisk, ignoreBlocks)`.
    #[allow(clippy::too_many_arguments)]
    pub fn save_structure(
        level: &mut ServerLevel,
        structure_name: &Identifier,
        pos: BlockPos,
        structure_size: Vec3i,
        ignore_entities: bool,
        author: &str,
        save_to_disk: bool,
        ignore_blocks: &[Block],
    ) -> bool {
        let manager = level.structure_template_manager().clone();
        // `getOrCreate` + in-place `fillFromWorld` / `setAuthor`: templates are immutable once cached, so a modified copy
        // replaces the cache entry
        let mut template: StructureTemplate = (*manager.get_or_create(structure_name)).clone();
        let mut ignored: Vec<Block> = ignore_blocks.to_vec();
        ignored.push(Block::STRUCTURE_VOID);
        if structure_size.x >= 1 && structure_size.y >= 1 && structure_size.z >= 1 {
            let corner2 = pos.offset_vec(&structure_size).offset(-1, -1, -1);
            let min_corner = BlockPos::new(pos.x().min(corner2.x()), pos.y().min(corner2.y()), pos.z().min(corner2.z()));
            let max_corner = BlockPos::new(pos.x().max(corner2.x()), pos.y().max(corner2.y()), pos.z().max(corner2.z()));
            // `Level.getBlockEntity` promotes pending chunk NBT; the read-only template scan below does not
            for p in BlockPos::between_closed(&min_corner, &max_corner) {
                if level.get_block_state(p).has_block_entity() {
                    level.get_block_entity_immediate(p);
                }
            }
            let entities = (!ignore_entities).then(|| collect_template_entities(level, &min_corner, &max_corner));
            template.fill_from_world(&*level, pos, structure_size, entities, &ignored);
        }
        template.set_author(author);
        manager.put(structure_name, Arc::new(template));
        if save_to_disk { manager.save(structure_name) } else { true }
    }

    /// `placeStructureIfSameSize(level)` of the structure block at `pos`.
    pub fn place_structure_if_same_size(level: &mut ServerLevel, pos: BlockPos) -> bool {
        let Some((mode, name, size)) = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| (be.mode, be.structure_name.clone(), be.structure_size)) else {
            return false;
        };
        let (StructureMode::Load, Some(name)) = (mode, name) else { return false };
        let Some(template) = level.structure_template_manager().get(&name) else { return false };
        if template.get_size() == size {
            Self::place_structure_template(level, pos, &template);
            true
        } else {
            Self::load_structure_info_from(level, pos, &template);
            false
        }
    }

    /// `loadStructureInfo(level)`.
    pub fn load_structure_info(level: &mut ServerLevel, pos: BlockPos) -> bool {
        match Self::get_structure_template(level, pos) {
            Some(template) => {
                Self::load_structure_info_from(level, pos, &template);
                true
            }
            None => false,
        }
    }

    /// `loadStructureInfo(template)`.
    fn load_structure_info_from(level: &mut ServerLevel, pos: BlockPos, template: &StructureTemplate) {
        level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |level, be| {
            be.author = if template.get_author().is_empty() { String::new() } else { template.get_author().to_owned() };
            be.structure_size = template.get_size();
            level.block_entity_set_changed(pos, be.get_block_state());
        });
    }

    /// `placeStructure(level)`.
    pub fn place_structure(level: &mut ServerLevel, pos: BlockPos) {
        if let Some(template) = Self::get_structure_template(level, pos) {
            Self::place_structure_template(level, pos, &template);
        }
    }

    /// `getStructureTemplate(level)`.
    fn get_structure_template(level: &mut ServerLevel, pos: BlockPos) -> Option<Arc<StructureTemplate>> {
        let name = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.structure_name.clone())??;
        level.structure_template_manager().get(&name)
    }

    /// `placeStructure(level, template)`.
    fn place_structure_template(level: &mut ServerLevel, pos: BlockPos, template: &StructureTemplate) {
        Self::load_structure_info_from(level, pos, template);
        let Some((mirror, rotation, ignore_entities, strict, integrity, seed, offset)) = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| {
            (be.mirror, be.rotation, be.ignore_entities, be.strict, be.integrity, be.seed, be.structure_pos)
        }) else {
            return;
        };
        let mut settings = StructurePlaceSettings::new();
        settings.set_mirror(mirror).set_rotation(rotation).set_ignore_entities(ignore_entities).set_known_shape(strict);
        if integrity < 1.0 {
            settings.clear_processors().add_processor(Arc::new(BlockRotProcessor::new(None, mth::clamp_f32(integrity, 0.0, 1.0)))).set_random(Some(SharedRandom::new(create_random(seed))));
        }
        let place_pos = pos.offset_vec(&Vec3i::new(offset.x(), offset.y(), offset.z()));
        let mut random = create_random(seed);
        template.place_in_world(level, place_pos, place_pos, &settings, &mut *random, 2 | if strict { 816 } else { 0 });
    }

    /// `unloadStructure()`.
    pub fn unload_structure(level: &mut ServerLevel, pos: BlockPos) {
        if let Some(Some(name)) = level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| be.structure_name.clone()) {
            level.structure_template_manager().remove(&name);
        }
    }

    /// `isStructureLoadable()`.
    pub fn is_structure_loadable(level: &mut ServerLevel, pos: BlockPos) -> bool {
        match level.with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, be| (be.mode, be.structure_name.clone())) {
            Some((StructureMode::Load, Some(name))) => level.structure_template_manager().get(&name).is_some(),
            _ => false,
        }
    }
}

/// `fillEntityList`: the non-player entities in the box as `(position, painting block position, saved NBT)`.
fn collect_template_entities(level: &ServerLevel, min_corner: &BlockPos, max_corner: &BlockPos) -> Vec<(Vec3, Option<BlockPos>, CompoundTag)> {
    let bb = AABB::encapsulating_full_blocks(min_corner, max_corner);
    let ids = level.get_entities_of_class(&bb, |_| true, |e| !e.is_player());
    let lookup = |id| level.entity_manager.get_any(id);
    let mut result = Vec::new();
    for id in ids {
        let Some(entity) = level.entity_manager.get_any(id) else { continue };
        // `Entity.save(output)` leaves the output empty for entities that are not saved on their own
        let nbt = crate::world::entity::entity::save(entity, &lookup).unwrap_or_default();
        let position = entity.base().position();
        let painting_pos = (entity.get_type() == mc_data::EntityType::PAINTING)
            .then(|| nbt.get_int_array("block_pos").filter(|p| p.len() == 3).map(|p| BlockPos::new(p[0], p[1], p[2])))
            .flatten();
        result.push((position, painting_pos, nbt));
    }
    result
}

impl BlockEntity for StructureBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.set_structure_name(tag.get_string_or("name", ""));
        self.author = tag.get_string_or("author", "").to_owned();
        self.meta_data = tag.get_string_or("metadata", "").to_owned();
        let x_offset = mth::clamp_i32(tag.get_int_or("posX", 0), -MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS);
        let y_offset = mth::clamp_i32(tag.get_int_or("posY", 1), -MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS);
        let z_offset = mth::clamp_i32(tag.get_int_or("posZ", 0), -MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS);
        self.structure_pos = BlockPos::new(x_offset, y_offset, z_offset);
        let width = mth::clamp_i32(tag.get_int_or("sizeX", 0), 0, MAX_SIZE_PER_AXIS);
        let height = mth::clamp_i32(tag.get_int_or("sizeY", 0), 0, MAX_SIZE_PER_AXIS);
        let depth = mth::clamp_i32(tag.get_int_or("sizeZ", 0), 0, MAX_SIZE_PER_AXIS);
        self.structure_size = Vec3i::new(width, height, depth);
        self.rotation = tag.get("rotation").and_then(|t| t.as_string()).and_then(rotation_by_name).unwrap_or(Rotation::None);
        self.mirror = tag.get("mirror").and_then(|t| t.as_string()).and_then(mirror_by_name).unwrap_or(Mirror::None);
        self.mode = tag.get("mode").and_then(|t| t.as_string()).and_then(mode_by_name).unwrap_or(StructureMode::Data);
        self.ignore_entities = tag.get_boolean_or("ignoreEntities", true);
        self.strict = tag.get_boolean_or("strict", false);
        self.powered = tag.get_boolean_or("powered", false);
        self.show_air = tag.get_boolean_or("showair", false);
        self.show_bounding_box = tag.get_boolean_or("showboundingbox", true);
        self.integrity = tag.get_float_or("integrity", 1.0);
        self.seed = tag.get_long_or("seed", 0);
    }

    /// `loadAdditional` with `updateBlockState()`: the block state follows the loaded mode.
    fn load_additional_in_level(&mut self, level: &mut ServerLevel, tag: &CompoundTag) -> Option<BlockState> {
        self.load_additional(tag);
        self.block_state_for_mode(level)
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_string("name", self.get_structure_name());
        tag.put_string("author", self.author.clone());
        tag.put_string("metadata", self.meta_data.clone());
        tag.put_int("posX", self.structure_pos.x());
        tag.put_int("posY", self.structure_pos.y());
        tag.put_int("posZ", self.structure_pos.z());
        tag.put_int("sizeX", self.structure_size.x);
        tag.put_int("sizeY", self.structure_size.y);
        tag.put_int("sizeZ", self.structure_size.z);
        tag.put_string("rotation", rotation_name(self.rotation));
        tag.put_string("mirror", mirror_name(self.mirror));
        tag.put_string("mode", mode_name(self.mode));
        tag.put_boolean("ignoreEntities", self.ignore_entities);
        tag.put_boolean("strict", self.strict);
        tag.put_boolean("powered", self.powered);
        tag.put_boolean("showair", self.show_air);
        tag.put_boolean("showboundingbox", self.show_bounding_box);
        tag.put_float("integrity", self.integrity);
        tag.put_long("seed", self.seed);
    }

    /// `getUpdateTag`: `saveCustomOnly`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static STRUCTURE_BLOCK: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::STRUCTURE_BLOCK, create: |pos, state| Box::new(StructureBlockEntity::new(pos, state)) };
