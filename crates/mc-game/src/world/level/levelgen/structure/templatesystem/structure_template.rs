//! `StructureTemplate`: a saved block/entity structure (`structure/<id>.nbt`) with its palettes, NBT (de)serialization,
//! `fillFromWorld` (structure blocks) and `placeInWorld` (generation).
//!
//! Differences from the Java object model (no observable effect on generated worlds):
//! * `placeInWorld` runs on a [`WorldGenLevel`] (what every structure piece places into); block entities are handed to
//!   the level as raw NBT (`id`/`x`/`y`/`z` added), entities as saved NBT through `add_fresh_entity_nbt`.
//! * The `DataFixer` step of loading is not ported: templates are read as written by the current data version (every
//!   template of the vanilla pack is). See `StructureTemplateManager`.
//! * `Mob.finalizeSpawn` for `finalizeEntities` settings is not performed (it needs live mob classes).

use super::structure_place_settings::StructurePlaceSettings;
use crate::world::block_entity::BlockEntity;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers::update_from_neighbour_shapes;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{LevelAccessor, LevelReader};
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::{BitSetDiscreteVoxelShape, DiscreteVoxelShape};
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::{self, RandomSource};
use mc_core::rotation::Rotation;
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState, block_flags};
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

/// `SharedConstants.getCurrentVersion().dataVersion()` (`NbtUtils.addCurrentDataVersion`).
pub const CURRENT_DATA_VERSION: i32 = mc_world::level_storage::WORLD_VERSION;

/// `StructureTemplate.StructureBlockInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureBlockInfo {
    pub pos: BlockPos,
    pub state: BlockState,
    pub nbt: Option<CompoundTag>,
}

impl StructureBlockInfo {
    pub fn new(pos: BlockPos, state: BlockState, nbt: Option<CompoundTag>) -> StructureBlockInfo {
        StructureBlockInfo { pos, state, nbt }
    }
}

/// `StructureTemplate.StructureEntityInfo`.
#[derive(Clone, Debug)]
pub struct StructureEntityInfo {
    pub pos: Vec3,
    pub block_pos: BlockPos,
    pub nbt: CompoundTag,
}

/// `JigsawBlockEntity.JointType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JointType {
    Rollable,
    Aligned,
}

impl JointType {
    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            JointType::Rollable => "rollable",
            JointType::Aligned => "aligned",
        }
    }

    pub fn by_name(name: &str) -> Option<JointType> {
        [JointType::Rollable, JointType::Aligned].into_iter().find(|t| t.get_serialized_name() == name)
    }
}

/// `StructureTemplate.JigsawBlockInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct JigsawBlockInfo {
    pub pos: BlockPos,
    pub state: BlockState,
    pub joint_type: JointType,
    /// `@Nullable` (`FeaturePoolElement` passes `null`); [`JigsawBlockInfo::parse`] always yields `Some`.
    pub name: Option<Identifier>,
    /// `ResourceKey<StructureTemplatePool>` (`Pools.EMPTY` when absent).
    pub pool: Identifier,
    pub target: Identifier,
    pub placement_priority: i32,
    pub selection_priority: i32,
}

/// `JigsawBlockEntity.EMPTY_ID` / `Pools.EMPTY`.
fn empty_id() -> Identifier {
    Identifier::parse("minecraft:empty").expect("valid identifier")
}

/// `nbt.read(key, Identifier.CODEC)`: a string tag holding a valid identifier.
fn read_identifier(nbt: &CompoundTag, key: &str) -> Option<Identifier> {
    nbt.get_string(key).and_then(|s| Identifier::read(s).ok())
}

impl JigsawBlockInfo {
    /// `JigsawBlockInfo.parse(info)`; panics when the info has no NBT (`Objects.requireNonNull`).
    pub fn parse(info: &StructureBlockInfo) -> JigsawBlockInfo {
        let nbt = info.nbt.as_ref().unwrap_or_else(|| panic!("{info:?} nbt was null"));
        JigsawBlockInfo {
            pos: info.pos,
            state: info.state,
            joint_type: get_joint_type(nbt, info.state),
            name: Some(read_identifier(nbt, "name").unwrap_or_else(empty_id)),
            pool: read_identifier(nbt, "pool").unwrap_or_else(empty_id),
            target: read_identifier(nbt, "target").unwrap_or_else(empty_id),
            placement_priority: nbt.get_int_or("placement_priority", 0),
            selection_priority: nbt.get_int_or("selection_priority", 0),
        }
    }

    /// `withInfo(pos, state)`.
    pub fn with_info(&self, pos: BlockPos, state: BlockState) -> JigsawBlockInfo {
        JigsawBlockInfo { pos, state, ..self.clone() }
    }
}

/// `JigsawBlock.getFrontFacing(state)`: the front of the `orientation` property.
pub fn jigsaw_front_facing(state: BlockState) -> Direction {
    let name = state.get_property_str("orientation").expect("jigsaw orientation");
    mc_core::direction::FrontAndTop::by_name(name).expect("valid jigsaw orientation").front()
}

/// `StructureTemplate.getJointType(nbt, state)`.
pub fn get_joint_type(nbt: &CompoundTag, state: BlockState) -> JointType {
    nbt.get_string("joint").and_then(JointType::by_name).unwrap_or_else(|| get_default_joint_type(state))
}

/// `StructureTemplate.getDefaultJointType(state)`.
pub fn get_default_joint_type(state: BlockState) -> JointType {
    if jigsaw_front_facing(state).get_axis().is_horizontal() { JointType::Aligned } else { JointType::Rollable }
}

/// `StructureTemplate.Palette`.
pub struct Palette {
    blocks: Vec<StructureBlockInfo>,
    cache: Mutex<HashMap<Block, Arc<[StructureBlockInfo]>>>,
    cached_jigsaws: OnceLock<Vec<JigsawBlockInfo>>,
}

impl Palette {
    fn new(blocks: Vec<StructureBlockInfo>) -> Palette {
        Palette { blocks, cache: Mutex::new(HashMap::new()), cached_jigsaws: OnceLock::new() }
    }

    /// `jigsaws()`.
    pub fn jigsaws(&self) -> &[JigsawBlockInfo] {
        self.cached_jigsaws.get_or_init(|| self.blocks_of(jigsaw_block()).iter().map(JigsawBlockInfo::parse).collect())
    }

    /// `blocks()`.
    pub fn blocks(&self) -> &[StructureBlockInfo] {
        &self.blocks
    }

    /// `blocks(filter)`.
    pub fn blocks_of(&self, filter: Block) -> Arc<[StructureBlockInfo]> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.entry(filter).or_insert_with(|| self.blocks.iter().filter(|b| b.state.block() == filter).cloned().collect()).clone()
    }
}

fn jigsaw_block() -> Block {
    Block::by_name("minecraft:jigsaw").expect("jigsaw block")
}

fn air() -> BlockState {
    Block::by_name("minecraft:air").expect("air block").default_state()
}

/// `StructureTemplate.SimplePalette`: `IdMapper<BlockState>` (state -> id lookup keeps the last mapping, id -> state
/// keeps the last state mapped to the id).
#[derive(Default)]
struct SimplePalette {
    id_to_state: Vec<Option<BlockState>>,
    state_to_id: HashMap<BlockState, i32>,
    last_id: i32,
}

impl SimplePalette {
    fn id_for(&mut self, state: BlockState) -> i32 {
        if let Some(id) = self.state_to_id.get(&state) {
            return *id;
        }
        let id = self.last_id;
        self.last_id += 1;
        self.add_mapping(state, id);
        id
    }

    fn state_for(&self, index: i32) -> BlockState {
        usize::try_from(index).ok().and_then(|i| self.id_to_state.get(i).copied().flatten()).unwrap_or_else(air)
    }

    fn add_mapping(&mut self, state: BlockState, id: i32) {
        self.state_to_id.insert(state, id);
        let id = id as usize;
        if self.id_to_state.len() <= id {
            self.id_to_state.resize(id + 1, None);
        }
        self.id_to_state[id] = Some(state);
    }

    fn iter(&self) -> impl Iterator<Item = BlockState> + '_ {
        self.id_to_state.iter().flatten().copied()
    }
}

/// `NbtUtils.readBlockState(blocks, tag)`.
pub fn read_block_state(tag: &CompoundTag) -> BlockState {
    let Some(block) = tag.get_string("id").and_then(|s| Identifier::read(s).ok()).and_then(|id| Block::by_name(&id.to_string())) else {
        return air();
    };
    let mut result = block.default_state();
    if let Some(properties) = tag.get_compound("properties") {
        for (key, value) in properties.iter() {
            if let Tag::String(value) = value
                && let Some(updated) = result.with_property_str(key, value)
            {
                result = updated;
            } else if result.get_property_str(key).is_some() {
                log::warn!("Unable to read property: {key} with value: {value:?} for blockstate: {tag:?}");
            }
        }
    }
    result
}

/// `NbtUtils.writeBlockState(state)`.
pub fn write_block_state(state: BlockState) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", state.block().name());
    let mut properties = CompoundTag::new();
    for (def, index) in state.values() {
        properties.put_string(def.name, def.values[index]);
    }
    if !properties.is_empty() {
        tag.put("properties", properties);
    }
    tag
}

fn int_list(values: &[i32]) -> ListTag {
    let mut list = ListTag::new();
    for value in values {
        list.push(Tag::Int(*value));
    }
    list
}

fn double_list(values: &[f64]) -> ListTag {
    let mut list = ListTag::new();
    for value in values {
        list.push(Tag::Double(*value));
    }
    list
}

#[derive(Default)]
pub struct StructureTemplate {
    palettes: Vec<Palette>,
    entity_info_list: Vec<StructureEntityInfo>,
    size: Vec3i,
    author: String,
}

/// `addToLists(info, full, blockEntities, other)`.
fn add_to_lists(
    info: StructureBlockInfo,
    full_block_list: &mut Vec<StructureBlockInfo>,
    block_entities_list: &mut Vec<StructureBlockInfo>,
    other_blocks_list: &mut Vec<StructureBlockInfo>,
) {
    if info.nbt.is_some() {
        block_entities_list.push(info);
    } else if !info.state.block().data().has(block_flags::DYNAMIC_SHAPE) && info.state.is_collision_shape_full_block() {
        full_block_list.push(info);
    } else {
        other_blocks_list.push(info);
    }
}

/// `buildInfoList(full, blockEntities, other)`: each group sorted by (y, x, z), groups concatenated in that order.
fn build_info_list(
    mut full_block_list: Vec<StructureBlockInfo>,
    mut block_entities_list: Vec<StructureBlockInfo>,
    mut other_blocks_list: Vec<StructureBlockInfo>,
) -> Vec<StructureBlockInfo> {
    let key = |info: &StructureBlockInfo| (info.pos.y(), info.pos.x(), info.pos.z());
    full_block_list.sort_by_key(key);
    other_blocks_list.sort_by_key(key);
    block_entities_list.sort_by_key(key);
    let mut result = full_block_list;
    result.extend(other_blocks_list);
    result.extend(block_entities_list);
    result
}

impl StructureTemplate {
    pub fn new() -> StructureTemplate {
        StructureTemplate::default()
    }

    /// `getSize()`.
    pub fn get_size(&self) -> Vec3i {
        self.size
    }

    pub fn set_author(&mut self, author: &str) {
        self.author = author.to_owned();
    }

    /// `getAuthor()` (`"?"` for a template that never had one set).
    pub fn get_author(&self) -> &str {
        if self.author.is_empty() { "?" } else { &self.author }
    }

    pub fn palettes(&self) -> &[Palette] {
        &self.palettes
    }

    pub fn entity_info_list(&self) -> &[StructureEntityInfo] {
        &self.entity_info_list
    }

    /// `fillFromWorld(level, position, size, includeEntities, ignoreBlocks)`. Entities are collected by the caller
    /// (`level.getEntitiesOfClass(Entity.class, AABB.encapsulatingFullBlocks(min, max), not a player)`) and given as
    /// `(position, painting block pos if the entity is a painting, saved NBT)`.
    pub fn fill_from_world(
        &mut self,
        level: &dyn LevelReader,
        position: BlockPos,
        size: Vec3i,
        entities: Option<Vec<(Vec3, Option<BlockPos>, CompoundTag)>>,
        ignore_blocks: &[Block],
    ) {
        if size.x < 1 || size.y < 1 || size.z < 1 {
            return;
        }
        let corner2 = position.offset_vec(&size).offset(-1, -1, -1);
        let mut full_block_list = Vec::new();
        let mut block_entities_list = Vec::new();
        let mut other_blocks_list = Vec::new();
        let min_corner = BlockPos::new(position.x().min(corner2.x()), position.y().min(corner2.y()), position.z().min(corner2.z()));
        let max_corner = BlockPos::new(position.x().max(corner2.x()), position.y().max(corner2.y()), position.z().max(corner2.z()));
        self.size = size;
        for pos in BlockPos::between_closed(&min_corner, &max_corner) {
            let state = level.get_block_state(pos);
            if ignore_blocks.contains(&state.block()) {
                continue;
            }
            let relative_pos = pos.subtract(&min_corner);
            let nbt = level.get_block_entity(pos).map(save_with_id);
            add_to_lists(StructureBlockInfo::new(relative_pos, state, nbt), &mut full_block_list, &mut block_entities_list, &mut other_blocks_list);
        }
        let block_info_list = build_info_list(full_block_list, block_entities_list, other_blocks_list);
        self.palettes.clear();
        self.palettes.push(Palette::new(block_info_list));
        self.entity_info_list.clear();
        if let Some(entities) = entities {
            for (entity_pos, painting_pos, nbt) in entities {
                let pos = Vec3::new(entity_pos.x - f64::from(min_corner.x()), entity_pos.y - f64::from(min_corner.y()), entity_pos.z - f64::from(min_corner.z()));
                let block_pos = match painting_pos {
                    Some(painting) => painting.subtract(&min_corner),
                    None => BlockPos::containing(pos.x, pos.y, pos.z),
                };
                self.entity_info_list.push(StructureEntityInfo { pos, block_pos, nbt: nbt.copy() });
            }
        }
    }

    /// `filterBlocks(position, settings, block)` (absolute).
    pub fn filter_blocks(&self, position: BlockPos, settings: &StructurePlaceSettings, block: Block) -> Vec<StructureBlockInfo> {
        self.filter_blocks_abs(position, settings, block, true)
    }

    /// `filterBlocks(position, settings, block, absolute)`.
    pub fn filter_blocks_abs(&self, position: BlockPos, settings: &StructurePlaceSettings, block: Block, absolute: bool) -> Vec<StructureBlockInfo> {
        let mut result = Vec::new();
        let bounding_box = settings.get_bounding_box();
        if self.palettes.is_empty() {
            return result;
        }
        for block_info in settings.get_random_palette(&self.palettes, Some(position)).blocks_of(block).iter() {
            let block_pos = if absolute { calculate_relative_position(settings, block_info.pos).offset_vec(&position) } else { block_info.pos };
            if bounding_box.is_none_or(|bb| bb.is_inside(&block_pos)) {
                result.push(StructureBlockInfo::new(block_pos, block_info.state.rotate(settings.get_rotation()), block_info.nbt.clone()));
            }
        }
        result
    }

    /// `getJigsaws(position, rotation)`.
    pub fn get_jigsaws(&self, position: BlockPos, rotation: Rotation) -> Vec<JigsawBlockInfo> {
        if self.palettes.is_empty() {
            return Vec::new();
        }
        let mut settings = StructurePlaceSettings::new();
        settings.set_rotation(rotation);
        settings
            .get_random_palette(&self.palettes, Some(position))
            .jigsaws()
            .iter()
            .map(|jigsaw| jigsaw.with_info(calculate_relative_position(&settings, jigsaw.pos).offset_vec(&position), jigsaw.state.rotate(settings.get_rotation())))
            .collect()
    }

    /// `calculateConnectedPosition(settings1, connection1, settings2, connection2)`.
    pub fn calculate_connected_position(&self, settings1: &StructurePlaceSettings, connection1: BlockPos, settings2: &StructurePlaceSettings, connection2: BlockPos) -> BlockPos {
        let marker_pos1 = calculate_relative_position(settings1, connection1);
        let marker_pos2 = calculate_relative_position(settings2, connection2);
        marker_pos1.subtract(&marker_pos2)
    }

    /// `placeInWorld(level, position, referencePos, settings, random, updateMode)`.
    pub fn place_in_world(
        &self,
        level: &mut dyn WorldGenLevel,
        position: BlockPos,
        reference_pos: BlockPos,
        settings: &StructurePlaceSettings,
        random: &mut dyn RandomSource,
        update_mode: u32,
    ) -> bool {
        if self.palettes.is_empty() {
            return false;
        }
        let block_info_list = settings.get_random_palette(&self.palettes, Some(position)).blocks();
        if (block_info_list.is_empty() && (settings.is_ignore_entities() || self.entity_info_list.is_empty())) || self.size.x < 1 || self.size.y < 1 || self.size.z < 1 {
            return false;
        }
        let bounding_box = settings.get_bounding_box();
        let waterlogging = settings.should_apply_waterlogging();
        let mut to_fill: Vec<BlockPos> = Vec::new();
        let mut locked_fluids: Vec<BlockPos> = Vec::new();
        let mut placed: Vec<(BlockPos, bool)> = Vec::with_capacity(block_info_list.len());
        let (mut min_x, mut min_y, mut min_z) = (i32::MAX, i32::MAX, i32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (i32::MIN, i32::MIN, i32::MIN);
        let processed_block_info_list = process_block_infos(level, position, reference_pos, settings, block_info_list);
        let barrier = Block::by_name("minecraft:barrier").expect("barrier block").default_state();
        for mut block_info in processed_block_info_list {
            let block_pos = block_info.pos;
            if !bounding_box.is_none_or(|bb| bb.is_inside(&block_pos)) {
                continue;
            }
            let previous_fluid_state = waterlogging.then(|| level.get_fluid_state(block_pos));
            let state = block_info.state.mirror(settings.get_mirror()).rotate(settings.get_rotation());
            if block_info.nbt.is_some() {
                level.set_block(block_pos, barrier, 820, 512);
            }
            if !level.set_block(block_pos, state, update_mode, 512) {
                continue;
            }
            min_x = min_x.min(block_pos.x());
            min_y = min_y.min(block_pos.y());
            min_z = min_z.min(block_pos.z());
            max_x = max_x.max(block_pos.x());
            max_y = max_y.max(block_pos.y());
            max_z = max_z.max(block_pos.z());
            placed.push((block_pos, block_info.nbt.is_some()));
            if let Some(nbt) = block_info.nbt.as_mut()
                && state.has_block_entity()
            {
                if is_randomizable_container(state.block()) {
                    nbt.put_long("LootTableSeed", random.next_long());
                }
                // `blockEntity.loadWithComponents(nbt)`: the level stores the block entity data (`id`, `x`, `y`, `z`
                // are the metadata `saveWithFullMetadata` adds)
                let mut tag = nbt.clone();
                tag.put_int("x", block_pos.x());
                tag.put_int("y", block_pos.y());
                tag.put_int("z", block_pos.z());
                level.set_block_entity_nbt(tag);
            }
            if let Some(previous_fluid_state) = previous_fluid_state {
                if state.get_fluid_state().is_source() {
                    locked_fluids.push(block_pos);
                } else if super::liquid_block_container::is_liquid_block_container(state) {
                    super::liquid_block_container::place_liquid(level, block_pos, state, previous_fluid_state);
                    if !previous_fluid_state.is_source() {
                        to_fill.push(block_pos);
                    }
                }
            }
        }
        let mut filled = true;
        const DIRECTIONS: [Direction; 5] = [Direction::Up, Direction::North, Direction::East, Direction::South, Direction::West];
        while filled && !to_fill.is_empty() {
            filled = false;
            let mut i = 0;
            while i < to_fill.len() {
                let pos = to_fill[i];
                let mut to_place = level.get_fluid_state(pos);
                let mut d = 0;
                while d < DIRECTIONS.len() && !to_place.is_source() {
                    let neighbor_pos = pos.relative(DIRECTIONS[d]);
                    let neighbor = level.get_fluid_state(neighbor_pos);
                    if neighbor.is_source() && !locked_fluids.contains(&neighbor_pos) {
                        to_place = neighbor;
                    }
                    d += 1;
                }
                if to_place.is_source() {
                    let state = level.get_block_state(pos);
                    if super::liquid_block_container::is_liquid_block_container(state) {
                        super::liquid_block_container::place_liquid(level, pos, state, to_place);
                        filled = true;
                        to_fill.remove(i);
                        continue;
                    }
                }
                i += 1;
            }
        }
        if min_x <= max_x {
            if !settings.get_known_shape() {
                let mut shape = BitSetDiscreteVoxelShape::new(max_x - min_x + 1, max_y - min_y + 1, max_z - min_z + 1);
                for (block_pos, _) in &placed {
                    shape.fill(block_pos.x() - min_x, block_pos.y() - min_y, block_pos.z() - min_z);
                }
                update_shape_at_edge(level, update_mode, &DiscreteVoxelShape::from(shape), min_x, min_y, min_z);
            }
            for (block_pos, _has_nbt) in &placed {
                let block_pos = *block_pos;
                if !settings.get_known_shape() {
                    let state = level.get_block_state(block_pos);
                    let new_state = update_from_neighbour_shapes(state, level, block_pos);
                    if state != new_state {
                        level.set_block(block_pos, new_state, (update_mode & !1) | 16, 512);
                    }
                    level.update_neighbors_at(block_pos, new_state.block());
                }
                // `blockEntity.setChanged()` has no effect on a world generation region's chunk
            }
        }
        if !settings.is_ignore_entities() {
            self.place_entities(level, position, settings.get_mirror(), settings.get_rotation(), settings.get_rotation_pivot(), bounding_box);
        }
        true
    }

    fn place_entities(&self, level: &mut dyn WorldGenLevel, position: BlockPos, mirror: Mirror, rotation: Rotation, pivot: BlockPos, bounding_box: Option<BoundingBox>) {
        for entity_info in &self.entity_info_list {
            let block_pos = transform(entity_info.block_pos, mirror, rotation, pivot).offset_vec(&position);
            if !bounding_box.is_none_or(|bb| bb.is_inside(&block_pos)) {
                continue;
            }
            let mut tag = entity_info.nbt.copy();
            let relative_pos = transform_vec3(entity_info.pos, mirror, rotation, pivot);
            let pos = relative_pos.add(Vec3::new(f64::from(position.x()), f64::from(position.y()), f64::from(position.z())));
            tag.put("Pos", double_list(&[pos.x, pos.y, pos.z]));
            tag.remove("UUID");
            if tag.contains("block_pos") {
                tag.put_int_array("block_pos", vec![block_pos.x(), block_pos.y(), block_pos.z()]);
            }
            // `EntityType.create` + `rotate(rotation)` / `mirror(mirror)` / `snapTo` (the base `Entity` rotation rules)
            let (old_y_rot, x_rot) = read_rotation(&tag);
            let angle = wrap_degrees(old_y_rot);
            let rotated = match rotation {
                Rotation::Clockwise180 => angle + 180.0,
                Rotation::Counterclockwise90 => angle + 270.0,
                Rotation::Clockwise90 => angle + 90.0,
                Rotation::None => angle,
            };
            let mirrored = match mirror {
                Mirror::FrontBack => -angle,
                Mirror::LeftRight => 180.0 - angle,
                Mirror::None => angle,
            };
            let mut y_rot = rotated;
            y_rot += mirrored - old_y_rot;
            let mut rotation_tag = ListTag::new();
            rotation_tag.push(Tag::Float(if y_rot.is_finite() { y_rot } else { old_y_rot }));
            rotation_tag.push(Tag::Float(x_rot));
            tag.put("Rotation", rotation_tag);
            // the entity constructor draws a fresh random UUID (`Mth.createInsecureUUID(entity.random)`)
            let uuid = mth::create_insecure_uuid(&mut random::create_new()).as_u64_pair();
            tag.put_int_array("UUID", vec![(uuid.0 >> 32) as i32, uuid.0 as i32, (uuid.1 >> 32) as i32, uuid.1 as i32]);
            level.add_fresh_entity_nbt(tag);
        }
    }

    /// `getSize(rotation)`.
    pub fn get_size_rotated(&self, rotation: Rotation) -> Vec3i {
        match rotation {
            Rotation::Counterclockwise90 | Rotation::Clockwise90 => Vec3i::new(self.size.z, self.size.y, self.size.x),
            _ => self.size,
        }
    }

    /// `getZeroPositionWithTransform(zeroPos, mirror, rotation)`.
    pub fn get_zero_position_with_transform(&self, zero_pos: BlockPos, mirror: Mirror, rotation: Rotation) -> BlockPos {
        get_zero_position_with_transform(zero_pos, mirror, rotation, self.size.x, self.size.z)
    }

    /// `getBoundingBox(settings, position)`.
    pub fn get_bounding_box(&self, settings: &StructurePlaceSettings, position: BlockPos) -> BoundingBox {
        self.get_bounding_box_with(position, settings.get_rotation(), settings.get_rotation_pivot(), settings.get_mirror())
    }

    /// `getBoundingBox(position, rotation, pivot, mirror)`.
    pub fn get_bounding_box_with(&self, position: BlockPos, rotation: Rotation, pivot: BlockPos, mirror: Mirror) -> BoundingBox {
        get_bounding_box(position, rotation, pivot, mirror, self.size)
    }

    /// `save(tag)`.
    pub fn save(&self, mut tag: CompoundTag) -> CompoundTag {
        if self.palettes.is_empty() {
            tag.put("blocks", ListTag::new());
            tag.put("palette", ListTag::new());
        } else {
            let mut palettes = vec![SimplePalette::default()];
            for _ in 1..self.palettes.len() {
                palettes.push(SimplePalette::default());
            }
            let mut block_list = ListTag::new();
            let main_palette_blocks = self.palettes[0].blocks();
            for (i, block_info) in main_palette_blocks.iter().enumerate() {
                let mut block_tag = CompoundTag::new();
                block_tag.put("pos", int_list(&[block_info.pos.x(), block_info.pos.y(), block_info.pos.z()]));
                let id = palettes[0].id_for(block_info.state);
                block_tag.put_int("state", id);
                if let Some(nbt) = &block_info.nbt {
                    block_tag.put("nbt", nbt.clone());
                }
                block_list.push(block_tag);
                for p in 1..self.palettes.len() {
                    palettes[p].add_mapping(self.palettes[p].blocks()[i].state, id);
                }
            }
            tag.put("blocks", block_list);
            let palette_to_list = |palette: &SimplePalette| {
                let mut list = ListTag::new();
                for state in palette.iter() {
                    list.push(write_block_state(state));
                }
                list
            };
            if palettes.len() == 1 {
                tag.put("palette", palette_to_list(&palettes[0]));
            } else {
                let mut list_list = ListTag::new();
                for palette in &palettes {
                    list_list.push(palette_to_list(palette));
                }
                tag.put("palettes", list_list);
            }
        }
        let mut entity_list = ListTag::new();
        for entity_info in &self.entity_info_list {
            let mut entity_tag = CompoundTag::new();
            entity_tag.put("pos", double_list(&[entity_info.pos.x, entity_info.pos.y, entity_info.pos.z]));
            entity_tag.put("blockPos", int_list(&[entity_info.block_pos.x(), entity_info.block_pos.y(), entity_info.block_pos.z()]));
            entity_tag.put("nbt", entity_info.nbt.clone());
            entity_list.push(entity_tag);
        }
        tag.put("entities", entity_list);
        tag.put("size", int_list(&[self.size.x, self.size.y, self.size.z]));
        mc_nbt::nbt_utils::add_data_version(&mut tag, CURRENT_DATA_VERSION);
        tag
    }

    /// `load(blockLookup, tag)` (the tag must already be at the current data version).
    pub fn load(&mut self, tag: &CompoundTag) {
        self.palettes.clear();
        self.entity_info_list.clear();
        let size_tag = tag.get_list_or_empty("size");
        self.size = Vec3i::new(size_tag.get_int_or(0, 0), size_tag.get_int_or(1, 0), size_tag.get_int_or(2, 0));
        let block_list = tag.get_list_or_empty("blocks");
        if let Some(palette_list_list) = tag.get_list("palettes") {
            for p in 0..palette_list_list.len() {
                self.load_palette(&palette_list_list.get_list_or_empty(p), block_list);
            }
        } else {
            self.load_palette(tag.get_list_or_empty("palette"), block_list);
        }
        for entity_tag in tag.get_list_or_empty("entities").compounds() {
            let pos_tag = entity_tag.get_list_or_empty("pos");
            let pos = Vec3::new(pos_tag.get_double_or(0, 0.0), pos_tag.get_double_or(1, 0.0), pos_tag.get_double_or(2, 0.0));
            let block_pos_tag = entity_tag.get_list_or_empty("blockPos");
            let block_pos = BlockPos::new(block_pos_tag.get_int_or(0, 0), block_pos_tag.get_int_or(1, 0), block_pos_tag.get_int_or(2, 0));
            if let Some(nbt) = entity_tag.get_compound("nbt") {
                self.entity_info_list.push(StructureEntityInfo { pos, block_pos, nbt: nbt.clone() });
            }
        }
    }

    fn load_palette(&mut self, palette_list: &ListTag, block_list: &ListTag) {
        let mut palette = SimplePalette::default();
        for i in 0..palette_list.len() {
            palette.add_mapping(read_block_state(&palette_list.get_compound_or_empty(i)), i as i32);
        }
        let mut full_block_list = Vec::new();
        let mut block_entities_list = Vec::new();
        let mut other_blocks_list = Vec::new();
        for block_tag in block_list.compounds() {
            let pos_tag = block_tag.get_list_or_empty("pos");
            let pos = BlockPos::new(pos_tag.get_int_or(0, 0), pos_tag.get_int_or(1, 0), pos_tag.get_int_or(2, 0));
            let state = palette.state_for(block_tag.get_int_or("state", 0));
            let nbt = block_tag.get_compound("nbt").cloned();
            add_to_lists(StructureBlockInfo::new(pos, state, nbt), &mut full_block_list, &mut block_entities_list, &mut other_blocks_list);
        }
        self.palettes.push(Palette::new(build_info_list(full_block_list, block_entities_list, other_blocks_list)));
    }
}

/// `saveWithId` of a live block entity.
fn save_with_id(block_entity: &dyn BlockEntity) -> CompoundTag {
    let mut tag = CompoundTag::new();
    block_entity.save_additional(&mut tag);
    tag.put_string("id", block_entity.get_type().name());
    tag
}

/// Whether the block's block entity implements `RandomizableContainer` (chest, trapped chest, barrel, crafter,
/// decorated pot, dispenser, dropper, hopper, shulker box).
fn is_randomizable_container(block: Block) -> bool {
    const TYPES: [&str; 9] = [
        "minecraft:chest",
        "minecraft:trapped_chest",
        "minecraft:barrel",
        "minecraft:crafter",
        "minecraft:decorated_pot",
        "minecraft:dispenser",
        "minecraft:dropper",
        "minecraft:hopper",
        "minecraft:shulker_box",
    ];
    static BLOCKS: LazyLock<Vec<Block>> = LazyLock::new(|| {
        TYPES.iter().filter_map(|name| BlockEntityType::by_name(name)).flat_map(|ty| ty.data().valid_blocks.iter().copied()).collect()
    });
    BLOCKS
        .contains(&block)
}

fn read_rotation(tag: &CompoundTag) -> (f32, f32) {
    match tag.get_list("Rotation") {
        Some(list) if list.len() == 2 => (list.get_float_or(0, 0.0), list.get_float_or(1, 0.0)),
        _ => (0.0, 0.0),
    }
}

/// `Mth.wrapDegrees(float)`.
fn wrap_degrees(degrees: f32) -> f32 {
    mth::wrap_degrees_f32(degrees)
}

/// `StructureTemplate.updateShapeAtEdge(level, updateMode, shape, startX, startY, startZ)`.
pub fn update_shape_at_edge(level: &mut dyn LevelAccessor, update_mode: u32, shape: &DiscreteVoxelShape, start_x: i32, start_y: i32, start_z: i32) {
    shape.for_all_faces(&mut |direction, x, y, z| {
        let pos = BlockPos::new(start_x + x, start_y + y, start_z + z);
        let neighbor_pos = pos.relative(direction);
        let state = level.get_block_state(pos);
        let neighbor_state = level.get_block_state(neighbor_pos);
        let new_state = state.update_shape(level, pos, direction, neighbor_pos, neighbor_state);
        if state != new_state {
            level.set_block(pos, new_state, update_mode & !1, 512);
        }
        let new_neighbor_state = neighbor_state.update_shape(level, neighbor_pos, direction.get_opposite(), pos, new_state);
        if neighbor_state != new_neighbor_state {
            level.set_block(neighbor_pos, new_neighbor_state, update_mode & !1, 512);
        }
    });
}

/// `StructureTemplate.processBlockInfos(level, position, referencePos, settings, blockInfoList)`.
pub fn process_block_infos(
    level: &dyn WorldGenLevel,
    position: BlockPos,
    reference_pos: BlockPos,
    settings: &StructurePlaceSettings,
    block_info_list: &[StructureBlockInfo],
) -> Vec<StructureBlockInfo> {
    let mut original_block_info_list: Vec<StructureBlockInfo> = Vec::new();
    let mut processed_block_info_list: Vec<StructureBlockInfo> = Vec::new();
    let process_only_in_current_chunk = !settings.get_processors().iter().any(|p| p.evaluates_entire_piece_state());
    let chunk_bb = settings.get_bounding_box();
    for block_info in block_info_list {
        let block_pos = calculate_relative_position(settings, block_info.pos).offset_vec(&position);
        if !process_only_in_current_chunk || chunk_bb.is_none_or(|bb| bb.is_inside(&block_pos)) {
            let mut processed_block_info = Some(StructureBlockInfo::new(block_pos, block_info.state, block_info.nbt.clone()));
            for processor in settings.get_processors() {
                let Some(info) = processed_block_info else { break };
                processed_block_info = processor.process_block(level, position, reference_pos, block_info.pos, info, settings);
            }
            if let Some(info) = processed_block_info {
                processed_block_info_list.push(info);
                original_block_info_list.push(block_info.clone());
            }
        }
    }
    for processor in settings.get_processors() {
        processed_block_info_list = processor.finalize_processing(level, position, reference_pos, &original_block_info_list, processed_block_info_list, settings);
    }
    processed_block_info_list
}

/// `calculateRelativePosition(settings, pos)`.
pub fn calculate_relative_position(settings: &StructurePlaceSettings, pos: BlockPos) -> BlockPos {
    transform(pos, settings.get_mirror(), settings.get_rotation(), settings.get_rotation_pivot())
}

/// `transform(pos, mirror, rotation, pivot)` (blocks).
pub fn transform(pos: BlockPos, mirror: Mirror, rotation: Rotation, pivot: BlockPos) -> BlockPos {
    let (mut x, y, mut z) = (pos.x(), pos.y(), pos.z());
    let mut was_mirrored = true;
    match mirror {
        Mirror::LeftRight => z = z.wrapping_neg(),
        Mirror::FrontBack => x = x.wrapping_neg(),
        Mirror::None => was_mirrored = false,
    }
    let pivot_x = pivot.x();
    let pivot_z = pivot.z();
    match rotation {
        Rotation::Counterclockwise90 => BlockPos::new(pivot_x.wrapping_sub(pivot_z).wrapping_add(z), y, pivot_x.wrapping_add(pivot_z).wrapping_sub(x)),
        Rotation::Clockwise90 => BlockPos::new(pivot_x.wrapping_add(pivot_z).wrapping_sub(z), y, pivot_z.wrapping_sub(pivot_x).wrapping_add(x)),
        Rotation::Clockwise180 => BlockPos::new(pivot_x.wrapping_add(pivot_x).wrapping_sub(x), y, pivot_z.wrapping_add(pivot_z).wrapping_sub(z)),
        Rotation::None => {
            if was_mirrored {
                BlockPos::new(x, y, z)
            } else {
                pos
            }
        }
    }
}

/// `transform(pos, mirror, rotation, pivot)` (entity positions).
pub fn transform_vec3(pos: Vec3, mirror: Mirror, rotation: Rotation, pivot: BlockPos) -> Vec3 {
    let (mut x, y, mut z) = (pos.x, pos.y, pos.z);
    let mut was_mirrored = true;
    match mirror {
        Mirror::LeftRight => z = 1.0 - z,
        Mirror::FrontBack => x = 1.0 - x,
        Mirror::None => was_mirrored = false,
    }
    // the Java int arithmetic of the pivot terms happens before the widening to double
    let (px, pz) = (pivot.x(), pivot.z());
    let pxmz = f64::from(px.wrapping_sub(pz));
    let pzmx = f64::from(pz.wrapping_sub(px));
    let pxz1 = f64::from(px.wrapping_add(pz).wrapping_add(1));
    match rotation {
        Rotation::Counterclockwise90 => Vec3::new(pxmz + z, y, pxz1 - x),
        Rotation::Clockwise90 => Vec3::new(pxz1 - z, y, pzmx + x),
        Rotation::Clockwise180 => Vec3::new(f64::from(px.wrapping_add(px).wrapping_add(1)) - x, y, f64::from(pz.wrapping_add(pz).wrapping_add(1)) - z),
        Rotation::None => {
            if was_mirrored {
                Vec3::new(x, y, z)
            } else {
                pos
            }
        }
    }
}

/// `getZeroPositionWithTransform(zeroPos, mirror, rotation, sizeX, sizeZ)`.
pub fn get_zero_position_with_transform(zero_pos: BlockPos, mirror: Mirror, rotation: Rotation, size_x: i32, size_z: i32) -> BlockPos {
    let size_x = size_x.wrapping_sub(1);
    let size_z = size_z.wrapping_sub(1);
    let mirror_delta_x = if mirror == Mirror::FrontBack { size_x } else { 0 };
    let mirror_delta_z = if mirror == Mirror::LeftRight { size_z } else { 0 };
    match rotation {
        Rotation::Counterclockwise90 => zero_pos.offset(mirror_delta_z, 0, size_x.wrapping_sub(mirror_delta_x)),
        Rotation::Clockwise90 => zero_pos.offset(size_z.wrapping_sub(mirror_delta_z), 0, mirror_delta_x),
        Rotation::Clockwise180 => zero_pos.offset(size_x.wrapping_sub(mirror_delta_x), 0, size_z.wrapping_sub(mirror_delta_z)),
        Rotation::None => zero_pos.offset(mirror_delta_x, 0, mirror_delta_z),
    }
}

/// `getBoundingBox(position, rotation, pivot, mirror, size)`.
pub fn get_bounding_box(position: BlockPos, rotation: Rotation, pivot: BlockPos, mirror: Mirror, size: Vec3i) -> BoundingBox {
    let delta = Vec3i::new(size.x.wrapping_sub(1), size.y.wrapping_sub(1), size.z.wrapping_sub(1));
    let corner1 = transform(BlockPos::ZERO, mirror, rotation, pivot);
    let corner2 = transform(BlockPos::ZERO.offset_vec(&delta), mirror, rotation, pivot);
    *BoundingBox::from_corners(&corner1, &corner2).move_vec(&position)
}
