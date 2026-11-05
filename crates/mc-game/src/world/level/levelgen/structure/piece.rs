//! `StructurePiece` and `StructurePieceType` (+ `StructurePieceSerializationContext`).
//!
//! A piece is a trait object. Shared state of the vanilla base class lives in [`PieceBase`] (bounding box, orientation,
//! mirror/rotation, gen depth, type); every protected helper of `StructurePiece` (`getWorldX/Y/Z`, `placeBlock`,
//! `generateBox`, `createChest`, ...) is a default method of the trait and uses `self.base()` plus the overridable
//! hooks ([`StructurePiece::can_be_replaced`], [`StructurePiece::is_replaceable_by_structures`]).
//!
//! A piece type registers itself from its own file:
//! ```ignore
//! #[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
//! pub static MINE_SHAFT_ROOM: StructurePieceType = StructurePieceType { id: "msroom", load: |ctx, tag| Ok(Box::new(MineShaftRoom::load(ctx, tag)?)) };
//! ```
//! (`id` is the lower-cased path of the vanilla `BuiltInRegistries.STRUCTURE_PIECE` key) and constructs its base with
//! `PieceBase::new(&MINE_SHAFT_ROOM, gen_depth, bounding_box)`.

use super::pieces::StructurePiecesBuilder;
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::block_entity_data::set_block_entity_loot_table;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::{Axis, Direction, Plane};
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_nbt::{CompoundTag, Tag};
use mc_world::heightmap::HeightmapType;
use mc_world::ticks::TickPriority;
use mc_worldgen::beardifier::StructurePieceKind;
use std::any::Any;
use std::sync::LazyLock;

/// `StructurePieceSerializationContext(resourceManager, registryAccess, structureTemplateManager)`: the level's
/// [`GenEnv`] carries the registries and (`structure_template_manager`) the template manager.
pub struct StructurePieceSerializationContext<'a> {
    pub env: &'a GenEnv,
}

/// `StructurePieceType`: registry id + the loader of the piece class (`StructurePieceType.load`). A loader error is
/// vanilla's exception while loading a piece (`PiecesContainer.load` logs it and skips the piece).
pub struct StructurePieceType {
    /// Lower-case path of the `minecraft:` registry key (`setPieceId` lower-cases the constant's id).
    pub id: &'static str,
    pub load: fn(&StructurePieceSerializationContext<'_>, &CompoundTag) -> Result<Box<dyn StructurePiece>, String>,
}

impl StructurePieceType {
    /// `BuiltInRegistries.STRUCTURE_PIECE.getKey(type).toString()`.
    pub fn registry_key(&self) -> String {
        format!("minecraft:{}", self.id)
    }

    /// `BuiltInRegistries.STRUCTURE_PIECE.getValue(id)` for a `minecraft:` id.
    pub fn by_key(namespace: &str, path: &str) -> Option<&'static StructurePieceType> {
        (namespace == "minecraft").then(|| STRUCTURE_PIECE_TYPES.iter().find(|t| t.id == path)).flatten()
    }
}

impl PartialEq for StructurePieceType {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

#[linkme::distributed_slice]
pub static STRUCTURE_PIECE_TYPES: [StructurePieceType];

/// The state of vanilla's `StructurePiece` fields.
#[derive(Clone, Debug)]
pub struct PieceBase {
    pub piece_type: &'static StructurePieceType,
    pub bounding_box: BoundingBox,
    orientation: Option<Direction>,
    mirror: Mirror,
    rotation: Rotation,
    pub gen_depth: i32,
}

impl std::fmt::Debug for StructurePieceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StructurePieceType({})", self.id)
    }
}

impl PieceBase {
    /// `StructurePiece(type, genDepth, boundingBox)` (no orientation: rotation/mirror `NONE`).
    pub fn new(piece_type: &'static StructurePieceType, gen_depth: i32, bounding_box: BoundingBox) -> PieceBase {
        PieceBase { piece_type, bounding_box, orientation: None, mirror: Mirror::None, rotation: Rotation::None, gen_depth }
    }

    /// `StructurePiece(type, tag)`: `GD`, `BB`, `O`. `BB` must be an int array (or list) of exactly 6 ints.
    pub fn load(piece_type: &'static StructurePieceType, tag: &CompoundTag) -> Result<PieceBase, String> {
        let bounding_box = read_bounding_box(tag.get("BB")).ok_or_else(|| "No value present".to_owned())?;
        let mut base = PieceBase::new(piece_type, tag.get_int_or("GD", 0), bounding_box);
        let orientation = tag.get_int_or("O", 0);
        base.set_orientation(if orientation == -1 { None } else { Some(Direction::from_2d_data_value(orientation)) });
        Ok(base)
    }

    pub fn orientation(&self) -> Option<Direction> {
        self.orientation
    }

    /// `setOrientation(orientation)`: also derives mirror and rotation.
    pub fn set_orientation(&mut self, orientation: Option<Direction>) {
        self.orientation = orientation;
        (self.mirror, self.rotation) = match orientation {
            None => (Mirror::None, Rotation::None),
            Some(Direction::South) => (Mirror::LeftRight, Rotation::None),
            Some(Direction::West) => (Mirror::LeftRight, Rotation::Clockwise90),
            Some(Direction::East) => (Mirror::None, Rotation::Clockwise90),
            Some(_) => (Mirror::None, Rotation::None),
        };
    }

    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    pub fn mirror(&self) -> Mirror {
        self.mirror
    }

    /// `createTag` head: `id`, `BB`, `O`, `GD`.
    fn write_header(&self, tag: &mut CompoundTag) {
        tag.put_string("id", self.piece_type.registry_key());
        tag.put_int_array("BB", bounding_box_to_ints(&self.bounding_box));
        tag.put_int("O", self.orientation.map_or(-1, Direction::get_2d_data_value));
        tag.put_int("GD", self.gen_depth);
    }
}

/// `BoundingBox.CODEC` encode: `[minX, minY, minZ, maxX, maxY, maxZ]`.
pub fn bounding_box_to_ints(bb: &BoundingBox) -> Vec<i32> {
    vec![bb.min_x(), bb.min_y(), bb.min_z(), bb.max_x(), bb.max_y(), bb.max_z()]
}

/// `BoundingBox.CODEC` decode (`Codec.INT_STREAM` over an `IntArrayTag`/`ByteArrayTag`/`LongArrayTag` or a list of
/// numbers, then `Util.fixedSize(.., 6)`). Corrupt inverted bounds are normalised by the constructor.
pub fn read_bounding_box(tag: Option<&Tag>) -> Option<BoundingBox> {
    let ints: Vec<i32> = match tag? {
        Tag::IntArray(v) => v.to_vec(),
        Tag::ByteArray(v) => v.iter().map(|&b| i32::from(b)).collect(),
        Tag::LongArray(v) => v.iter().map(|&l| l as i32).collect(),
        Tag::List(list) => list.iter().map(|t| t.as_number().map(|n| n.int_value())).collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    match ints[..] {
        [a, b, c, d, e, f] => Some(BoundingBox::new(a, b, c, d, e, f)),
        _ => None,
    }
}

/// `StructurePiece.BlockSelector`.
pub trait BlockSelector {
    /// `next(random, worldX, worldY, worldZ, isEdge)`.
    fn next(&mut self, random: &mut dyn RandomSource, world_x: i32, world_y: i32, world_z: i32, is_edge: bool);
    /// `getNext()`.
    fn get_next(&self) -> BlockState;
}

/// `StructurePiece.CAVE_AIR`.
pub fn cave_air() -> BlockState {
    Block::CAVE_AIR.default_state()
}

fn shape_check_blocks() -> &'static [Block; 12] {
    static BLOCKS: LazyLock<[Block; 12]> = LazyLock::new(|| {
        [
            Block::NETHER_BRICK_FENCE,
            Block::TORCH,
            Block::WALL_TORCH,
            Block::OAK_FENCE,
            Block::SPRUCE_FENCE,
            Block::DARK_OAK_FENCE,
            Block::PALE_OAK_FENCE,
            Block::ACACIA_FENCE,
            Block::BIRCH_FENCE,
            Block::JUNGLE_FENCE,
            Block::LADDER,
            Block::IRON_BARS,
        ]
    });
    &BLOCKS
}

/// `StructurePiece.makeBoundingBox(x, y, z, direction, width, height, depth)`.
pub fn make_bounding_box(x: i32, y: i32, z: i32, direction: Direction, width: i32, height: i32, depth: i32) -> BoundingBox {
    if direction.get_axis() == Axis::Z {
        BoundingBox::new(x, y, z, x + width - 1, y + height - 1, z + depth - 1)
    } else {
        BoundingBox::new(x, y, z, x + depth - 1, y + height - 1, z + width - 1)
    }
}

/// `StructurePiece.getRandomHorizontalDirection(random)`.
pub fn get_random_horizontal_direction(random: &mut dyn RandomSource) -> Direction {
    Plane::Horizontal.get_random_direction(random)
}

/// `StructurePiece.createBoundingBox(pieces)`; panics like vanilla's `IllegalStateException` without pieces.
pub fn create_bounding_box<'a>(pieces: impl IntoIterator<Item = &'a dyn StructurePiece>) -> BoundingBox {
    BoundingBox::encapsulating_boxes(pieces.into_iter().map(|p| p.base().bounding_box))
        .expect("Unable to calculate boundingbox without pieces")
}

/// `StructurePiece.findCollisionPiece(pieces, box)`.
pub fn find_collision_piece<'a>(pieces: impl IntoIterator<Item = &'a dyn StructurePiece>, bounding_box: &BoundingBox) -> Option<&'a dyn StructurePiece> {
    pieces.into_iter().find(|p| p.base().bounding_box.intersects(bounding_box))
}

/// `StructurePiece.reorient(level, blockPos, blockState)` for a chest-like `FACING` state.
pub fn reorient(level: &dyn BlockGetter, block_pos: BlockPos, block_state: BlockState) -> BlockState {
    let mut solid_neighbor: Option<Direction> = None;
    for direction in Plane::Horizontal.iter() {
        let state = level.get_block_state(block_pos.relative(direction));
        if state.block() == Block::CHEST {
            return block_state;
        }
        if state.is_solid_render() {
            if solid_neighbor.is_some() {
                solid_neighbor = None;
                break;
            }
            solid_neighbor = Some(direction);
        }
    }
    if let Some(solid) = solid_neighbor {
        return block_state.set(props::HORIZONTAL_FACING, solid.get_opposite());
    }
    let mut lock_dir = block_state.get(props::HORIZONTAL_FACING);
    let mut relative = block_pos.relative(lock_dir);
    if level.get_block_state(relative).is_solid_render() {
        lock_dir = lock_dir.get_opposite();
        relative = block_pos.relative(lock_dir);
    }
    if level.get_block_state(relative).is_solid_render() {
        lock_dir = lock_dir.get_clockwise();
        relative = block_pos.relative(lock_dir);
    }
    if level.get_block_state(relative).is_solid_render() {
        lock_dir = lock_dir.get_opposite();
    }
    block_state.set(props::HORIZONTAL_FACING, lock_dir)
}

/// `RandomizableContainer.setLootTable(table, seed)` on the raw block entity NBT the level holds at `pos`
/// (`LootTable`, and `LootTableSeed` only when non-zero: `trySaveLootTable`). Returns whether a block entity exists.
pub fn set_loot_table(level: &mut dyn WorldGenLevel, pos: BlockPos, loot_table: &str, seed: i64) -> bool {
    let Some(existing) = level.get_block_entity_nbt(pos) else { return false };
    let mut tag = existing.clone();
    tag.remove("Items");
    tag.put_string("LootTable", loot_table);
    if seed != 0 {
        tag.put_long("LootTableSeed", seed);
    } else {
        tag.remove("LootTableSeed");
    }
    level.remove_block_entity_nbt(pos);
    level.set_block_entity_nbt(tag);
    true
}

/// `StructurePiece`.
pub trait StructurePiece: Any + Send + Sync {
    fn base(&self) -> &PieceBase;
    fn base_mut(&mut self) -> &mut PieceBase;

    /// `addAdditionalSaveData(context, tag)`: the subclass fields (the base writes `id`, `BB`, `O`, `GD` first).
    fn add_additional_save_data(&self, context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag);

    /// `addChildren(startPiece, builder, random)`. Vanilla passes the piece itself as `startPiece` for the first call:
    /// that is `start_piece == None` here (`self` is the start piece); otherwise `Some(start)` is another piece of the
    /// builder. Run it through [`StructurePiecesBuilder::add_piece_and_children`] so the piece stays visible to
    /// `find_collision_piece` while its children are generated.
    fn add_children(&mut self, _start_piece: Option<&mut dyn StructurePiece>, _builder: &mut StructurePiecesBuilder, _random: &mut dyn RandomSource) {}

    /// `postProcess(level, structureManager, generator, random, chunkBB, chunkPos, referencePos)`.
    #[allow(clippy::too_many_arguments)]
    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        structure_manager: &StructureManager,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        chunk_pos: ChunkPos,
        reference_pos: BlockPos,
    );

    /// What the terrain `Beardifier` reads of this piece (`PoolElementStructurePiece` overrides it).
    fn beardifier_kind(&self) -> StructurePieceKind {
        StructurePieceKind::Plain
    }

    /// `createTag(context)`.
    fn create_tag(&self, context: &StructurePieceSerializationContext<'_>) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.base().write_header(&mut tag);
        self.add_additional_save_data(context, &mut tag);
        tag
    }

    fn get_bounding_box(&self) -> BoundingBox {
        self.base().bounding_box
    }

    fn get_gen_depth(&self) -> i32 {
        self.base().gen_depth
    }

    fn set_gen_depth(&mut self, gen_depth: i32) {
        self.base_mut().gen_depth = gen_depth;
    }

    /// `isCloseToChunk(pos, distance)`.
    fn is_close_to_chunk(&self, pos: ChunkPos, distance: i32) -> bool {
        let cx = pos.get_min_block_x();
        let cz = pos.get_min_block_z();
        self.base().bounding_box.intersects_xz(cx - distance, cz - distance, cx + 15 + distance, cz + 15 + distance)
    }

    /// `getLocatorPosition()`.
    fn get_locator_position(&self) -> BlockPos {
        self.base().bounding_box.get_center()
    }

    /// `move(dx, dy, dz)`.
    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.base_mut().bounding_box.move_(dx, dy, dz);
    }

    fn get_orientation(&self) -> Option<Direction> {
        self.base().orientation
    }

    fn set_orientation(&mut self, orientation: Option<Direction>) {
        self.base_mut().set_orientation(orientation);
    }

    fn get_rotation(&self) -> Rotation {
        self.base().rotation
    }

    fn get_mirror(&self) -> Mirror {
        self.base().mirror
    }

    /// `getWorldPos(x, y, z)`.
    fn get_world_pos(&self, x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(self.get_world_x(x, z), self.get_world_y(y), self.get_world_z(x, z))
    }

    /// `getWorldX(x, z)`.
    fn get_world_x(&self, x: i32, z: i32) -> i32 {
        let base = self.base();
        match base.orientation {
            None => x,
            Some(Direction::North | Direction::South) => base.bounding_box.min_x() + x,
            Some(Direction::West) => base.bounding_box.max_x() - z,
            Some(Direction::East) => base.bounding_box.min_x() + z,
            Some(_) => x,
        }
    }

    /// `getWorldY(y)`.
    fn get_world_y(&self, y: i32) -> i32 {
        let base = self.base();
        if base.orientation.is_none() { y } else { y + base.bounding_box.min_y() }
    }

    /// `getWorldZ(x, z)`.
    fn get_world_z(&self, x: i32, z: i32) -> i32 {
        let base = self.base();
        match base.orientation {
            None => z,
            Some(Direction::North) => base.bounding_box.max_z() - z,
            Some(Direction::South) => base.bounding_box.min_z() + z,
            Some(Direction::West | Direction::East) => base.bounding_box.min_z() + x,
            Some(_) => z,
        }
    }

    /// `placeBlock(level, blockState, x, y, z, chunkBB)`.
    fn place_block(&self, level: &mut dyn WorldGenLevel, block_state: BlockState, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) {
        let pos = self.get_world_pos(x, y, z);
        if !chunk_bb.is_inside(&pos) || !self.can_be_replaced(level, x, y, z, chunk_bb) {
            return;
        }
        let mut state = block_state;
        if self.base().mirror != Mirror::None {
            state = state.mirror(self.base().mirror);
        }
        if self.base().rotation != Rotation::None {
            state = state.rotate(self.base().rotation);
        }
        level.set_block_flags(pos, state, 2);
        let fluid_state = level.get_fluid_state(pos);
        if !fluid_state.is_empty() {
            level.schedule_fluid_tick(pos, fluid_state.fluid(), 0, TickPriority::Normal);
        }
        if shape_check_blocks().contains(&state.block()) {
            level.mark_pos_for_post_processing(pos);
        }
    }

    /// `canBeReplaced(level, x, y, z, chunkBB)` (overridable; vanilla's base answers `true`).
    fn can_be_replaced(&self, _level: &dyn LevelReader, _x: i32, _y: i32, _z: i32, _chunk_bb: &BoundingBox) -> bool {
        true
    }

    /// `getBlock(level, x, y, z, chunkBB)`: `AIR` outside the chunk box.
    fn get_block(&self, level: &dyn BlockGetter, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> BlockState {
        let pos = self.get_world_pos(x, y, z);
        if chunk_bb.is_inside(&pos) { level.get_block_state(pos) } else { Block::AIR.default_state() }
    }

    /// `isInterior(level, x, y, z, chunkBB)`.
    fn is_interior(&self, level: &dyn LevelReader, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> bool {
        let pos = self.get_world_pos(x, y + 1, z);
        chunk_bb.is_inside(&pos) && pos.y() < level.get_height_at(HeightmapType::OceanFloorWg, pos.x(), pos.z())
    }

    /// `generateAirBox(level, chunkBB, x0, y0, z0, x1, y1, z1)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_air_box(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        let air = Block::AIR.default_state();
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    self.place_block(level, air, x, y, z, chunk_bb);
                }
            }
        }
    }

    /// `generateBox(level, chunkBB, x0, y0, z0, x1, y1, z1, edgeBlock, fillBlock, skipAir)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_box(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        edge_block: BlockState,
        fill_block: BlockState,
        skip_air: bool,
    ) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if !skip_air || !self.get_block(level, x, y, z, chunk_bb).is_air() {
                        if y != y0 && y != y1 && x != x0 && x != x1 && z != z0 && z != z1 {
                            self.place_block(level, fill_block, x, y, z, chunk_bb);
                        } else {
                            self.place_block(level, edge_block, x, y, z, chunk_bb);
                        }
                    }
                }
            }
        }
    }

    /// `generateBox(level, chunkBB, boxBB, edgeBlock, fillBlock, skipAir)`.
    fn generate_box_bb(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, box_bb: &BoundingBox, edge_block: BlockState, fill_block: BlockState, skip_air: bool) {
        self.generate_box(
            level,
            chunk_bb,
            box_bb.min_x(),
            box_bb.min_y(),
            box_bb.min_z(),
            box_bb.max_x(),
            box_bb.max_y(),
            box_bb.max_z(),
            edge_block,
            fill_block,
            skip_air,
        );
    }

    /// `generateBox(level, chunkBB, x0, y0, z0, x1, y1, z1, skipAir, random, selector)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_box_selector(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        skip_air: bool,
        random: &mut dyn RandomSource,
        selector: &mut dyn BlockSelector,
    ) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if !skip_air || !self.get_block(level, x, y, z, chunk_bb).is_air() {
                        selector.next(random, x, y, z, y == y0 || y == y1 || x == x0 || x == x1 || z == z0 || z == z1);
                        self.place_block(level, selector.get_next(), x, y, z, chunk_bb);
                    }
                }
            }
        }
    }

    /// `generateBox(level, chunkBB, boxBB, skipAir, random, selector)`.
    fn generate_box_bb_selector(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        box_bb: &BoundingBox,
        skip_air: bool,
        random: &mut dyn RandomSource,
        selector: &mut dyn BlockSelector,
    ) {
        self.generate_box_selector(
            level,
            chunk_bb,
            box_bb.min_x(),
            box_bb.min_y(),
            box_bb.min_z(),
            box_bb.max_x(),
            box_bb.max_y(),
            box_bb.max_z(),
            skip_air,
            random,
            selector,
        );
    }

    /// `generateMaybeBox(level, chunkBB, random, probability, x0, y0, z0, x1, y1, z1, edge, fill, skipAir, hasToBeInside)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_maybe_box(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        random: &mut dyn RandomSource,
        probability: f32,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        edge_block: BlockState,
        fill_block: BlockState,
        skip_air: bool,
        has_to_be_inside: bool,
    ) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if !(random.next_float() > probability)
                        && (!skip_air || !self.get_block(level, x, y, z, chunk_bb).is_air())
                        && (!has_to_be_inside || self.is_interior(level, x, y, z, chunk_bb))
                    {
                        if y != y0 && y != y1 && x != x0 && x != x1 && z != z0 && z != z1 {
                            self.place_block(level, fill_block, x, y, z, chunk_bb);
                        } else {
                            self.place_block(level, edge_block, x, y, z, chunk_bb);
                        }
                    }
                }
            }
        }
    }

    /// `maybeGenerateBlock(level, chunkBB, random, probability, x, y, z, blockState)`.
    #[allow(clippy::too_many_arguments)]
    fn maybe_generate_block(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, random: &mut dyn RandomSource, probability: f32, x: i32, y: i32, z: i32, block_state: BlockState) {
        if random.next_float() < probability {
            self.place_block(level, block_state, x, y, z, chunk_bb);
        }
    }

    /// `generateUpperHalfSphere(level, chunkBB, x0, y0, z0, x1, y1, z1, fillBlock, skipAir)` (`float` maths).
    #[allow(clippy::too_many_arguments)]
    fn generate_upper_half_sphere(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, fill_block: BlockState, skip_air: bool) {
        let diag_x = (x1 - x0 + 1) as f32;
        let diag_y = (y1 - y0 + 1) as f32;
        let diag_z = (z1 - z0 + 1) as f32;
        let cx = x0 as f32 + diag_x / 2.0;
        let cz = z0 as f32 + diag_z / 2.0;
        for y in y0..=y1 {
            let normalized_y_distance = (y - y0) as f32 / diag_y;
            for x in x0..=x1 {
                let normalized_x_distance = (x as f32 - cx) / (diag_x * 0.5);
                for z in z0..=z1 {
                    let normalized_z_distance = (z as f32 - cz) / (diag_z * 0.5);
                    if !skip_air || !self.get_block(level, x, y, z, chunk_bb).is_air() {
                        let dist = normalized_x_distance * normalized_x_distance
                            + normalized_y_distance * normalized_y_distance
                            + normalized_z_distance * normalized_z_distance;
                        if dist <= 1.05 {
                            self.place_block(level, fill_block, x, y, z, chunk_bb);
                        }
                    }
                }
            }
        }
    }

    /// `fillColumnDown(level, blockState, x, startY, z, chunkBB)`.
    fn fill_column_down(&self, level: &mut dyn WorldGenLevel, block_state: BlockState, x: i32, start_y: i32, z: i32, chunk_bb: &BoundingBox) {
        let mut pos = self.get_world_pos(x, start_y, z);
        if chunk_bb.is_inside(&pos) {
            while self.is_replaceable_by_structures(level.get_block_state(pos)) && pos.y() > level.get_min_y() + 1 {
                level.set_block_flags(pos, block_state, 2);
                pos = pos.below();
            }
        }
    }

    /// `isReplaceableByStructures(state)`.
    fn is_replaceable_by_structures(&self, state: BlockState) -> bool {
        state.is_air() || state.liquid() || matches!(state.block(), Block::GLOW_LICHEN | Block::SEAGRASS | Block::TALL_SEAGRASS)
    }

    /// `createChest(level, chunkBB, random, x, y, z, lootTable)`.
    #[allow(clippy::too_many_arguments)]
    fn create_chest(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, random: &mut dyn RandomSource, x: i32, y: i32, z: i32, loot_table: &str) -> bool {
        self.create_chest_at(level, chunk_bb, random, self.get_world_pos(x, y, z), loot_table, None)
    }

    /// `createChest(level, chunkBB, random, pos, lootTable, blockState)`.
    fn create_chest_at(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, random: &mut dyn RandomSource, pos: BlockPos, loot_table: &str, block_state: Option<BlockState>) -> bool {
        if chunk_bb.is_inside(&pos) && level.get_block_state(pos).block() != Block::CHEST {
            let state = block_state.unwrap_or_else(|| reorient(level, pos, Block::CHEST.default_state()));
            level.set_block_flags(pos, state, 2);
            set_block_entity_loot_table(level, random, pos, loot_table);
            true
        } else {
            false
        }
    }

    /// `createDispenser(level, chunkBB, random, x, y, z, facing, lootTable)`.
    #[allow(clippy::too_many_arguments)]
    fn create_dispenser(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, random: &mut dyn RandomSource, x: i32, y: i32, z: i32, facing: Direction, loot_table: &str) -> bool {
        let pos = self.get_world_pos(x, y, z);
        if chunk_bb.is_inside(&pos) && level.get_block_state(pos).block() != Block::DISPENSER {
            self.place_block(level, Block::DISPENSER.default_state().set(props::FACING, facing), x, y, z, chunk_bb);
            set_block_entity_loot_table(level, random, pos, loot_table);
            true
        } else {
            false
        }
    }
}

impl dyn StructurePiece {
    pub fn downcast_ref<T: StructurePiece>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }

    pub fn downcast_mut<T: StructurePiece>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_TYPE: StructurePieceType = StructurePieceType { id: "test_piece", load: |_, _| Err("unused".to_owned()) };

    #[test]
    fn header_round_trip_and_orientation() {
        let mut base = PieceBase::new(&TEST_TYPE, 7, BoundingBox::new(-3, 10, 4, 12, 20, 40));
        base.set_orientation(Some(Direction::West));
        assert_eq!((base.mirror(), base.rotation()), (Mirror::LeftRight, Rotation::Clockwise90));
        let mut tag = CompoundTag::new();
        base.write_header(&mut tag);
        assert_eq!(tag.get_string("id"), Some("minecraft:test_piece"));
        assert_eq!(tag.get_int_array("BB"), Some(&[-3, 10, 4, 12, 20, 40][..]));
        assert_eq!(tag.get_int("O"), Some(Direction::West.get_2d_data_value()));
        let loaded = PieceBase::load(&TEST_TYPE, &tag).unwrap();
        assert_eq!(loaded.bounding_box, base.bounding_box);
        assert_eq!(loaded.orientation(), Some(Direction::West));
        assert_eq!(loaded.gen_depth, 7);
        tag.put_int("O", -1);
        assert_eq!(PieceBase::load(&TEST_TYPE, &tag).unwrap().orientation(), None);
        tag.remove("BB");
        assert!(PieceBase::load(&TEST_TYPE, &tag).is_err());
    }

    #[test]
    fn make_bounding_box_swaps_for_x_axis() {
        assert_eq!(make_bounding_box(0, 0, 0, Direction::North, 5, 3, 7), BoundingBox::new(0, 0, 0, 4, 2, 6));
        assert_eq!(make_bounding_box(0, 0, 0, Direction::East, 5, 3, 7), BoundingBox::new(0, 0, 0, 6, 2, 4));
    }
}
