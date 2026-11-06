//! `OceanMonumentPieces`: the ocean monument building (`MonumentBuilding`) and its rooms.
//!
//! Vanilla's abstract `OceanMonumentPiece` and its eleven room subclasses are one struct here ([`MonumentPiece`], the
//! subclass is [`PieceKind`]): the rooms differ only in their `postProcess`, and each kind keeps its own registered
//! `StructurePieceType` (`OMCR`, `OMDXR`, ...). The `RoomDefinition` graph is an arena ([`RoomGraph`]) shared by the
//! building's child pieces. Only `MonumentBuilding` is ever saved; its children are rebuilt from the building's seed
//! (`OceanMonumentStructure.regeneratePiecesAfterLoad`), so the child types never load from NBT.

use crate::world::level::levelgen::structure::structures::structure_entities;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext,
    StructurePieceType, make_bounding_box,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;
use std::ops::{Index, IndexMut};
use std::sync::Arc;

// `Direction.get3DDataValue()` of the directions the room graph indexes by.
const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

const GRIDROOM_SOURCE_INDEX: i32 = get_room_index(2, 0, 0);
const GRIDROOM_TOP_CONNECT_INDEX: i32 = get_room_index(2, 2, 0);
const GRIDROOM_LEFTWING_CONNECT_INDEX: i32 = get_room_index(0, 1, 0);
const GRIDROOM_RIGHTWING_CONNECT_INDEX: i32 = get_room_index(4, 1, 0);

/// `OceanMonumentPiece.getRoomIndex(roomX, roomY, roomZ)`.
const fn get_room_index(room_x: i32, room_y: i32, room_z: i32) -> i32 {
    room_y * 25 + room_z * 5 + room_x
}

fn base_gray() -> BlockState {
    Block::PRISMARINE.default_state()
}
fn base_light() -> BlockState {
    Block::PRISMARINE_BRICKS.default_state()
}
fn base_black() -> BlockState {
    Block::DARK_PRISMARINE.default_state()
}
/// `DOT_DECO_DATA`
fn dot_deco_data() -> BlockState {
    base_light()
}
fn lamp_block() -> BlockState {
    Block::SEA_LANTERN.default_state()
}
/// `FILL_BLOCK`
fn fill_block() -> BlockState {
    Block::WATER.default_state()
}

/// `RoomDefinition`.
#[derive(Debug)]
pub struct RoomDefinition {
    pub index: i32,
    /// Arena ids of the neighbours by `Direction.get3DDataValue()`.
    pub connections: [Option<usize>; 6],
    pub has_opening: [bool; 6],
    claimed: bool,
    is_source: bool,
    scan_index: i32,
}

impl RoomDefinition {
    fn new(room_index: i32) -> RoomDefinition {
        RoomDefinition {
            index: room_index,
            connections: [None; 6],
            has_opening: [false; 6],
            claimed: false,
            is_source: false,
            scan_index: 0,
        }
    }

    /// `isSpecial()`.
    fn is_special(&self) -> bool {
        self.index >= 75
    }

    /// `countOpenings()`.
    pub fn count_openings(&self) -> i32 {
        self.has_opening.iter().filter(|&&o| o).count() as i32
    }
}

/// All `RoomDefinition`s of one monument (vanilla's object graph as an arena).
#[derive(Debug, Default)]
pub struct RoomGraph {
    rooms: Vec<RoomDefinition>,
}

impl Index<usize> for RoomGraph {
    type Output = RoomDefinition;
    fn index(&self, id: usize) -> &RoomDefinition {
        &self.rooms[id]
    }
}

impl IndexMut<usize> for RoomGraph {
    fn index_mut(&mut self, id: usize) -> &mut RoomDefinition {
        &mut self.rooms[id]
    }
}

impl RoomGraph {
    fn add(&mut self, room_index: i32) -> usize {
        self.rooms.push(RoomDefinition::new(room_index));
        self.rooms.len() - 1
    }

    /// `setConnection(direction, definition)`.
    fn set_connection(&mut self, room: usize, direction: Direction, other: usize) {
        self.rooms[room].connections[direction.get_3d_data_value()] = Some(other);
        self.rooms[other].connections[direction.get_opposite().get_3d_data_value()] = Some(room);
    }

    /// `updateOpenings()`.
    fn update_openings(&mut self, room: usize) {
        let room = &mut self.rooms[room];
        for i in 0..6 {
            room.has_opening[i] = room.connections[i].is_some();
        }
    }

    /// `findSource(scanIndex)`.
    fn find_source(&mut self, room: usize, scan_index: i32) -> bool {
        if self.rooms[room].is_source {
            return true;
        }
        self.rooms[room].scan_index = scan_index;
        for i in 0..6 {
            if let Some(next) = self.rooms[room].connections[i] {
                if self.rooms[room].has_opening[i]
                    && self.rooms[next].scan_index != scan_index
                    && self.find_source(next, scan_index)
                {
                    return true;
                }
            }
        }
        false
    }

    fn conn(&self, room: usize, direction: usize) -> usize {
        self.rooms[room].connections[direction].expect("room connection")
    }
}

/// The `OceanMonumentPiece` subclasses that are rooms of the building (everything but `MonumentBuilding`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Core,
    DoubleX,
    DoubleXY,
    DoubleY,
    DoubleYZ,
    DoubleZ,
    Entry,
    Penthouse,
    Simple,
    SimpleTop,
    Wing,
}

impl PieceKind {
    fn piece_type(self) -> &'static StructurePieceType {
        match self {
            PieceKind::Core => &OMCR,
            PieceKind::DoubleX => &OMDXR,
            PieceKind::DoubleXY => &OMDXYR,
            PieceKind::DoubleY => &OMDYR,
            PieceKind::DoubleYZ => &OMDYZR,
            PieceKind::DoubleZ => &OMDZR,
            PieceKind::Entry => &OMENTRY,
            PieceKind::Penthouse => &OMPENTHOUSE,
            PieceKind::Simple => &OMSIMPLE,
            PieceKind::SimpleTop => &OMSIMPLET,
            PieceKind::Wing => &OMWR,
        }
    }

    /// The `roomWidth, roomHeight, roomDepth` the room constructors pass (`None`: bounding box given).
    fn room_size(self) -> Option<(i32, i32, i32)> {
        match self {
            PieceKind::Core => Some((2, 2, 2)),
            PieceKind::DoubleX => Some((2, 1, 1)),
            PieceKind::DoubleXY => Some((2, 2, 1)),
            PieceKind::DoubleY => Some((1, 2, 1)),
            PieceKind::DoubleYZ => Some((1, 2, 2)),
            PieceKind::DoubleZ => Some((1, 1, 2)),
            PieceKind::Entry | PieceKind::Simple | PieceKind::SimpleTop => Some((1, 1, 1)),
            PieceKind::Penthouse | PieceKind::Wing => None,
        }
    }
}

macro_rules! piece_types {
    ($($name:ident, $id:literal, $kind:expr;)*) => {$(
        #[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
        static $name: StructurePieceType = StructurePieceType { id: $id, load: |_, tag| MonumentPiece::load($kind, tag).map(|p| Box::new(p) as Box<dyn StructurePiece>) };
    )*};
}

piece_types! {
    OMCR, "omcr", PieceKind::Core;
    OMDXR, "omdxr", PieceKind::DoubleX;
    OMDXYR, "omdxyr", PieceKind::DoubleXY;
    OMDYR, "omdyr", PieceKind::DoubleY;
    OMDYZR, "omdyzr", PieceKind::DoubleYZ;
    OMDZR, "omdzr", PieceKind::DoubleZ;
    OMENTRY, "omentry", PieceKind::Entry;
    OMPENTHOUSE, "ompenthouse", PieceKind::Penthouse;
    OMSIMPLE, "omsimple", PieceKind::Simple;
    OMSIMPLET, "omsimplet", PieceKind::SimpleTop;
    OMWR, "omwr", PieceKind::Wing;
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
static OMB: StructurePieceType = StructurePieceType {
    id: "omb",
    load: |_, tag| MonumentBuilding::load(tag).map(|p| Box::new(p) as Box<dyn StructurePiece>),
};

/// Helpers of `OceanMonumentPiece` shared by the building and the rooms.
trait OceanMonumentPiece: StructurePiece {
    /// `generateWaterBox(level, chunkBB, x0, y0, z0, x1, y1, z1)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_water_box(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
    ) {
        let fill = fill_block();
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    let block = self.get_block(level, x, y, z, chunk_bb);
                    if !matches!(
                        block.block(),
                        Block::ICE | Block::PACKED_ICE | Block::BLUE_ICE | Block::WATER
                    ) {
                        if self.get_world_y(y) >= level.get_sea_level() && block != fill {
                            self.place_block(level, Block::AIR.default_state(), x, y, z, chunk_bb);
                        } else {
                            self.place_block(level, fill, x, y, z, chunk_bb);
                        }
                    }
                }
            }
        }
    }

    /// `generateDefaultFloor(level, chunkBB, xOff, zOff, downOpening)`.
    fn generate_default_floor(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x_off: i32,
        z_off: i32,
        down_opening: bool,
    ) {
        if down_opening {
            self.generate_box(
                level,
                chunk_bb,
                x_off,
                0,
                z_off,
                x_off + 2,
                0,
                z_off + 8 - 1,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 5,
                0,
                z_off,
                x_off + 8 - 1,
                0,
                z_off + 8 - 1,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 3,
                0,
                z_off,
                x_off + 4,
                0,
                z_off + 2,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 3,
                0,
                z_off + 5,
                x_off + 4,
                0,
                z_off + 8 - 1,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 3,
                0,
                z_off + 2,
                x_off + 4,
                0,
                z_off + 2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 3,
                0,
                z_off + 5,
                x_off + 4,
                0,
                z_off + 5,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 2,
                0,
                z_off + 3,
                x_off + 2,
                0,
                z_off + 4,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                x_off + 5,
                0,
                z_off + 3,
                x_off + 5,
                0,
                z_off + 4,
                base_light(),
                base_light(),
                false,
            );
        } else {
            self.generate_box(
                level,
                chunk_bb,
                x_off,
                0,
                z_off,
                x_off + 8 - 1,
                0,
                z_off + 8 - 1,
                base_gray(),
                base_gray(),
                false,
            );
        }
    }

    /// `generateBoxOnFillOnly(level, chunkBB, x0, y0, z0, x1, y1, z1, targetBlock)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_box_on_fill_only(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        target_block: BlockState,
    ) {
        let fill = fill_block();
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if self.get_block(level, x, y, z, chunk_bb) == fill {
                        self.place_block(level, target_block, x, y, z, chunk_bb);
                    }
                }
            }
        }
    }

    /// `chunkIntersects(chunkBB, x0, z0, x1, z1)`.
    fn chunk_intersects(&self, chunk_bb: &BoundingBox, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let wx0 = self.get_world_x(x0, z0);
        let wz0 = self.get_world_z(x0, z0);
        let wx1 = self.get_world_x(x1, z1);
        let wz1 = self.get_world_z(x1, z1);
        chunk_bb.intersects_xz(wx0.min(wx1), wz0.min(wz1), wx0.max(wx1), wz0.max(wz1))
    }

    /// `spawnElder(level, chunkBB, x, y, z)`: `EntityTypes.ELDER_GUARDIAN.create`, `heal`, `snapTo`, `finalizeSpawn` (which
    /// draws from the level's random) and `addFreshEntityWithPassengers`, as the saved entity NBT.
    fn spawn_elder(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x: i32,
        y: i32,
        z: i32,
    ) {
        let pos = self.get_world_pos(x, y, z);
        if chunk_bb.is_inside(&pos) {
            let tag = structure_entities::elder_guardian(level.random(), pos);
            level.add_fresh_entity_nbt(tag);
        }
    }
}

impl<T: StructurePiece + ?Sized> OceanMonumentPiece for T {}

/// `OceanMonumentPieces.OceanMonumentPiece` subclasses that are not `MonumentBuilding`.
pub struct MonumentPiece {
    base: PieceBase,
    kind: PieceKind,
    /// `roomDefinition`: the graph and the room's id in it (`None` for the box pieces and for loaded pieces).
    rooms: Option<Arc<RoomGraph>>,
    room: usize,
    /// `mainDesign` (`OceanMonumentSimpleRoom`, `OceanMonumentWingRoom`).
    main_design: i32,
}

impl MonumentPiece {
    /// `OceanMonumentPiece(type, genDepth, orientation, roomDefinition, roomWidth, roomHeight, roomDepth)`.
    fn new_room(
        kind: PieceKind,
        orientation: Direction,
        rooms: Arc<RoomGraph>,
        room: usize,
        main_design: i32,
    ) -> MonumentPiece {
        let (room_width, room_height, room_depth) = kind.room_size().expect("room piece");
        let bounding_box = make_room_bounding_box(
            orientation,
            rooms[room].index,
            room_width,
            room_height,
            room_depth,
        );
        let mut base = PieceBase::new(kind.piece_type(), 1, bounding_box);
        base.set_orientation(Some(orientation));
        MonumentPiece {
            base,
            kind,
            rooms: Some(rooms),
            room,
            main_design,
        }
    }

    /// `OceanMonumentPiece(type, orientation, genDepth, boundingBox)`.
    fn new_box(
        kind: PieceKind,
        orientation: Direction,
        bounding_box: BoundingBox,
        main_design: i32,
    ) -> MonumentPiece {
        let mut base = PieceBase::new(kind.piece_type(), 1, bounding_box);
        base.set_orientation(Some(orientation));
        MonumentPiece {
            base,
            kind,
            rooms: None,
            room: 0,
            main_design,
        }
    }

    /// `OceanMonumentPiece(type, tag)`. The room pieces are never saved by vanilla (their `roomDefinition` would be
    /// `null` and `postProcess` would crash), so only the box pieces load.
    fn load(kind: PieceKind, tag: &CompoundTag) -> Result<MonumentPiece, String> {
        if kind.room_size().is_some() {
            return Err("ocean monument room pieces are not saved".to_owned());
        }
        Ok(MonumentPiece {
            base: PieceBase::load(kind.piece_type(), tag)?,
            kind,
            rooms: None,
            room: 0,
            main_design: 0,
        })
    }

    fn room_graph(&self) -> (&RoomGraph, usize) {
        (
            self.rooms
                .as_deref()
                .expect("room piece has a room definition"),
            self.room,
        )
    }
}

/// `OceanMonumentPiece.makeBoundingBox(orientation, roomDefinition, roomWidth, roomHeight, roomDepth)`.
fn make_room_bounding_box(
    orientation: Direction,
    room_index: i32,
    room_width: i32,
    room_height: i32,
    room_depth: i32,
) -> BoundingBox {
    let room_x = room_index % 5;
    let room_z = room_index / 5 % 5;
    let room_y = room_index / 25;
    let mut bounding_box = make_bounding_box(
        0,
        0,
        0,
        orientation,
        room_width * 8,
        room_height * 4,
        room_depth * 8,
    );
    match orientation {
        Direction::North => {
            bounding_box.move_(room_x * 8, room_y * 4, -(room_z + room_depth) * 8 + 1)
        }
        Direction::South => bounding_box.move_(room_x * 8, room_y * 4, room_z * 8),
        Direction::West => {
            bounding_box.move_(-(room_z + room_depth) * 8 + 1, room_y * 4, room_x * 8)
        }
        _ => bounding_box.move_(room_z * 8, room_y * 4, room_x * 8),
    };
    bounding_box
}

impl StructurePiece for MonumentPiece {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        _tag: &mut CompoundTag,
    ) {
    }

    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        _reference_pos: BlockPos,
    ) {
        match self.kind {
            PieceKind::Core => self.post_process_core_room(level, random, chunk_bb),
            PieceKind::DoubleX => self.post_process_double_x_room(level, random, chunk_bb),
            PieceKind::DoubleXY => self.post_process_double_xy_room(level, random, chunk_bb),
            PieceKind::DoubleY => self.post_process_double_y_room(level, random, chunk_bb),
            PieceKind::DoubleYZ => self.post_process_double_yz_room(level, random, chunk_bb),
            PieceKind::DoubleZ => self.post_process_double_z_room(level, random, chunk_bb),
            PieceKind::Entry => self.post_process_entry_room(level, random, chunk_bb),
            PieceKind::Penthouse => self.post_process_penthouse(level, random, chunk_bb),
            PieceKind::Simple => self.post_process_simple_room(level, random, chunk_bb),
            PieceKind::SimpleTop => self.post_process_simple_top_room(level, random, chunk_bb),
            PieceKind::Wing => self.post_process_wing_room(level, random, chunk_bb),
        }
    }
}

#[allow(unused_variables, unused_mut, clippy::needless_late_init)]
impl MonumentPiece {
    fn post_process_core_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        self.generate_box_on_fill_only(level, chunk_bb, 1, 8, 0, 14, 8, 14, base_gray());
        let mut block = base_light();
        self.generate_box(level, chunk_bb, 0, 7, 0, 0, 7, 15, block, block, false);
        self.generate_box(level, chunk_bb, 15, 7, 0, 15, 7, 15, block, block, false);
        self.generate_box(level, chunk_bb, 1, 7, 0, 15, 7, 0, block, block, false);
        self.generate_box(level, chunk_bb, 1, 7, 15, 14, 7, 15, block, block, false);

        for yx in 1..=6 {
            block = base_light();
            if yx == 2 || yx == 6 {
                block = base_gray();
            }

            for x in (0..=15).step_by(15) {
                self.generate_box(level, chunk_bb, x, yx, 0, x, yx, 1, block, block, false);
                self.generate_box(level, chunk_bb, x, yx, 6, x, yx, 9, block, block, false);
                self.generate_box(level, chunk_bb, x, yx, 14, x, yx, 15, block, block, false);
            }

            self.generate_box(level, chunk_bb, 1, yx, 0, 1, yx, 0, block, block, false);
            self.generate_box(level, chunk_bb, 6, yx, 0, 9, yx, 0, block, block, false);
            self.generate_box(level, chunk_bb, 14, yx, 0, 14, yx, 0, block, block, false);
            self.generate_box(level, chunk_bb, 1, yx, 15, 14, yx, 15, block, block, false);
        }

        self.generate_box(
            level,
            chunk_bb,
            6,
            3,
            6,
            9,
            6,
            9,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            4,
            7,
            8,
            5,
            8,
            Block::GOLD_BLOCK.default_state(),
            Block::GOLD_BLOCK.default_state(),
            false,
        );

        for yx in (3..=6).step_by(3) {
            for x in (6..=9).step_by(3) {
                self.place_block(level, lamp_block(), x, yx, 6, chunk_bb);
                self.place_block(level, lamp_block(), x, yx, 9, chunk_bb);
            }
        }

        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            6,
            5,
            2,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            9,
            5,
            2,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            1,
            6,
            10,
            2,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            1,
            9,
            10,
            2,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            1,
            5,
            6,
            2,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            9,
            1,
            5,
            9,
            2,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            1,
            10,
            6,
            2,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            9,
            1,
            10,
            9,
            2,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            2,
            5,
            5,
            6,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            2,
            10,
            5,
            6,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            2,
            5,
            10,
            6,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            2,
            10,
            10,
            6,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            7,
            1,
            5,
            7,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            7,
            1,
            10,
            7,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            7,
            9,
            5,
            7,
            14,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            7,
            9,
            10,
            7,
            14,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            7,
            5,
            6,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            7,
            10,
            6,
            7,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            9,
            7,
            5,
            14,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            9,
            7,
            10,
            14,
            7,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            1,
            2,
            2,
            1,
            3,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            2,
            3,
            1,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            13,
            1,
            2,
            13,
            1,
            3,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            12,
            1,
            2,
            12,
            1,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            1,
            12,
            2,
            1,
            13,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            13,
            3,
            1,
            13,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            13,
            1,
            12,
            13,
            1,
            13,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            12,
            1,
            13,
            12,
            1,
            13,
            base_light(),
            base_light(),
            false,
        );
    }

    fn post_process_double_x_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        let mut east = g[room].connections[EAST].expect("room connection");
        let mut west = room;
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 8, 0, g[east].has_opening[DOWN]);
            self.generate_default_floor(level, chunk_bb, 0, 0, g[west].has_opening[DOWN]);
        }

        if g[west].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 4, 1, 7, 4, 6, base_gray());
        }

        if g[east].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 8, 4, 1, 14, 4, 6, base_gray());
        }

        self.generate_box(
            level,
            chunk_bb,
            0,
            3,
            0,
            0,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            15,
            3,
            0,
            15,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            0,
            15,
            3,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            7,
            14,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            2,
            0,
            0,
            2,
            7,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            15,
            2,
            0,
            15,
            2,
            7,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            2,
            0,
            15,
            2,
            0,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            2,
            7,
            14,
            2,
            7,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            0,
            0,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            15,
            1,
            0,
            15,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            0,
            15,
            1,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            7,
            14,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            0,
            10,
            1,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            2,
            0,
            9,
            2,
            3,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            3,
            0,
            10,
            3,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.place_block(level, lamp_block(), 6, 2, 3, chunk_bb);
        self.place_block(level, lamp_block(), 9, 2, 3, chunk_bb);
        if g[west].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 0, 4, 2, 0);
        }

        if g[west].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 7, 4, 2, 7);
        }

        if g[west].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 3, 0, 2, 4);
        }

        if g[east].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 11, 1, 0, 12, 2, 0);
        }

        if g[east].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 11, 1, 7, 12, 2, 7);
        }

        if g[east].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 15, 1, 3, 15, 2, 4);
        }
    }

    fn post_process_double_xy_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        let mut east = g[room].connections[EAST].expect("room connection");
        let mut west = room;
        let mut west_up = g[west].connections[UP].expect("room connection");
        let mut east_up = g[east].connections[UP].expect("room connection");
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 8, 0, g[east].has_opening[DOWN]);
            self.generate_default_floor(level, chunk_bb, 0, 0, g[west].has_opening[DOWN]);
        }

        if g[west_up].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 8, 1, 7, 8, 6, base_gray());
        }

        if g[east_up].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 8, 8, 1, 14, 8, 6, base_gray());
        }

        for y in 1..=7 {
            let mut block = base_light();
            if y == 2 || y == 6 {
                block = base_gray();
            }

            self.generate_box(level, chunk_bb, 0, y, 0, 0, y, 7, block, block, false);
            self.generate_box(level, chunk_bb, 15, y, 0, 15, y, 7, block, block, false);
            self.generate_box(level, chunk_bb, 1, y, 0, 15, y, 0, block, block, false);
            self.generate_box(level, chunk_bb, 1, y, 7, 14, y, 7, block, block, false);
        }

        self.generate_box(
            level,
            chunk_bb,
            2,
            1,
            3,
            2,
            7,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            2,
            4,
            7,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            5,
            4,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            13,
            1,
            3,
            13,
            7,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            11,
            1,
            2,
            12,
            7,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            11,
            1,
            5,
            12,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            3,
            5,
            3,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            1,
            3,
            10,
            3,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            7,
            2,
            10,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            5,
            2,
            5,
            7,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            5,
            2,
            10,
            7,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            5,
            5,
            5,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            5,
            5,
            10,
            7,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.place_block(level, base_light(), 6, 6, 2, chunk_bb);
        self.place_block(level, base_light(), 9, 6, 2, chunk_bb);
        self.place_block(level, base_light(), 6, 6, 5, chunk_bb);
        self.place_block(level, base_light(), 9, 6, 5, chunk_bb);
        self.generate_box(
            level,
            chunk_bb,
            5,
            4,
            3,
            6,
            4,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            9,
            4,
            3,
            10,
            4,
            4,
            base_light(),
            base_light(),
            false,
        );
        self.place_block(level, lamp_block(), 5, 4, 2, chunk_bb);
        self.place_block(level, lamp_block(), 5, 4, 5, chunk_bb);
        self.place_block(level, lamp_block(), 10, 4, 2, chunk_bb);
        self.place_block(level, lamp_block(), 10, 4, 5, chunk_bb);
        if g[west].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 0, 4, 2, 0);
        }

        if g[west].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 7, 4, 2, 7);
        }

        if g[west].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 3, 0, 2, 4);
        }

        if g[east].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 11, 1, 0, 12, 2, 0);
        }

        if g[east].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 11, 1, 7, 12, 2, 7);
        }

        if g[east].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 15, 1, 3, 15, 2, 4);
        }

        if g[west_up].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 5, 0, 4, 6, 0);
        }

        if g[west_up].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 5, 7, 4, 6, 7);
        }

        if g[west_up].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 5, 3, 0, 6, 4);
        }

        if g[east_up].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 11, 5, 0, 12, 6, 0);
        }

        if g[east_up].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 11, 5, 7, 12, 6, 7);
        }

        if g[east_up].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 15, 5, 3, 15, 6, 4);
        }
    }

    fn post_process_double_y_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 0, 0, g[room].has_opening[DOWN]);
        }

        let mut above = g[room].connections[UP].expect("room connection");
        if g[above].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 8, 1, 6, 8, 6, base_gray());
        }

        self.generate_box(
            level,
            chunk_bb,
            0,
            4,
            0,
            0,
            4,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            4,
            0,
            7,
            4,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            4,
            0,
            6,
            4,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            4,
            7,
            6,
            4,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            4,
            1,
            2,
            4,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            4,
            2,
            1,
            4,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            4,
            1,
            5,
            4,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            4,
            2,
            6,
            4,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            4,
            5,
            2,
            4,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            4,
            5,
            1,
            4,
            5,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            4,
            5,
            5,
            4,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            4,
            5,
            6,
            4,
            5,
            base_light(),
            base_light(),
            false,
        );
        let mut definition = room;

        for y in (1..=5).step_by(4) {
            let mut z = 0;
            if g[definition].has_opening[SOUTH] {
                self.generate_box(
                    level,
                    chunk_bb,
                    2,
                    y,
                    z,
                    2,
                    y + 2,
                    z,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    5,
                    y,
                    z,
                    5,
                    y + 2,
                    z,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    y + 2,
                    z,
                    4,
                    y + 2,
                    z,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    y,
                    z,
                    7,
                    y + 2,
                    z,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    y + 1,
                    z,
                    7,
                    y + 1,
                    z,
                    base_gray(),
                    base_gray(),
                    false,
                );
            }

            let mut var13 = 7;
            if g[definition].has_opening[NORTH] {
                self.generate_box(
                    level,
                    chunk_bb,
                    2,
                    y,
                    var13,
                    2,
                    y + 2,
                    var13,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    5,
                    y,
                    var13,
                    5,
                    y + 2,
                    var13,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    y + 2,
                    var13,
                    4,
                    y + 2,
                    var13,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    y,
                    var13,
                    7,
                    y + 2,
                    var13,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    y + 1,
                    var13,
                    7,
                    y + 1,
                    var13,
                    base_gray(),
                    base_gray(),
                    false,
                );
            }

            let mut x = 0;
            if g[definition].has_opening[WEST] {
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    y,
                    2,
                    x,
                    y + 2,
                    2,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    y,
                    5,
                    x,
                    y + 2,
                    5,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    y + 2,
                    3,
                    x,
                    y + 2,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    y,
                    0,
                    x,
                    y + 2,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    y + 1,
                    0,
                    x,
                    y + 1,
                    7,
                    base_gray(),
                    base_gray(),
                    false,
                );
            }

            let mut var14 = 7;
            if g[definition].has_opening[EAST] {
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    y,
                    2,
                    var14,
                    y + 2,
                    2,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    y,
                    5,
                    var14,
                    y + 2,
                    5,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    y + 2,
                    3,
                    var14,
                    y + 2,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    y,
                    0,
                    var14,
                    y + 2,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    y + 1,
                    0,
                    var14,
                    y + 1,
                    7,
                    base_gray(),
                    base_gray(),
                    false,
                );
            }

            definition = above;
        }
    }

    fn post_process_double_yz_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        let mut north = g[room].connections[NORTH].expect("room connection");
        let mut south = room;
        let mut north_up = g[north].connections[UP].expect("room connection");
        let mut south_up = g[south].connections[UP].expect("room connection");
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 0, 8, g[north].has_opening[DOWN]);
            self.generate_default_floor(level, chunk_bb, 0, 0, g[south].has_opening[DOWN]);
        }

        if g[south_up].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 8, 1, 6, 8, 7, base_gray());
        }

        if g[north_up].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 8, 8, 6, 8, 14, base_gray());
        }

        for y in 1..=7 {
            let mut block = base_light();
            if y == 2 || y == 6 {
                block = base_gray();
            }

            self.generate_box(level, chunk_bb, 0, y, 0, 0, y, 15, block, block, false);
            self.generate_box(level, chunk_bb, 7, y, 0, 7, y, 15, block, block, false);
            self.generate_box(level, chunk_bb, 1, y, 0, 6, y, 0, block, block, false);
            self.generate_box(level, chunk_bb, 1, y, 15, 6, y, 15, block, block, false);
        }

        for y in 1..=7 {
            let mut block = base_black();
            if y == 2 || y == 6 {
                block = lamp_block();
            }

            self.generate_box(level, chunk_bb, 3, y, 7, 4, y, 8, block, block, false);
        }

        if g[south].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 0, 4, 2, 0);
        }

        if g[south].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 7, 1, 3, 7, 2, 4);
        }

        if g[south].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 3, 0, 2, 4);
        }

        if g[north].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 15, 4, 2, 15);
        }

        if g[north].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 11, 0, 2, 12);
        }

        if g[north].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 7, 1, 11, 7, 2, 12);
        }

        if g[south_up].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 5, 0, 4, 6, 0);
        }

        if g[south_up].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 7, 5, 3, 7, 6, 4);
            self.generate_box(
                level,
                chunk_bb,
                5,
                4,
                2,
                6,
                4,
                5,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                1,
                2,
                6,
                3,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                1,
                5,
                6,
                3,
                5,
                base_light(),
                base_light(),
                false,
            );
        }

        if g[south_up].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 5, 3, 0, 6, 4);
            self.generate_box(
                level,
                chunk_bb,
                1,
                4,
                2,
                2,
                4,
                5,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                1,
                2,
                1,
                3,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                1,
                5,
                1,
                3,
                5,
                base_light(),
                base_light(),
                false,
            );
        }

        if g[north_up].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 5, 15, 4, 6, 15);
        }

        if g[north_up].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 5, 11, 0, 6, 12);
            self.generate_box(
                level,
                chunk_bb,
                1,
                4,
                10,
                2,
                4,
                13,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                1,
                10,
                1,
                3,
                10,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                1,
                13,
                1,
                3,
                13,
                base_light(),
                base_light(),
                false,
            );
        }

        if g[north_up].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 7, 5, 11, 7, 6, 12);
            self.generate_box(
                level,
                chunk_bb,
                5,
                4,
                10,
                6,
                4,
                13,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                1,
                10,
                6,
                3,
                10,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                1,
                13,
                6,
                3,
                13,
                base_light(),
                base_light(),
                false,
            );
        }
    }

    fn post_process_double_z_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        let mut north = g[room].connections[NORTH].expect("room connection");
        let mut south = room;
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 0, 8, g[north].has_opening[DOWN]);
            self.generate_default_floor(level, chunk_bb, 0, 0, g[south].has_opening[DOWN]);
        }

        if g[south].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 4, 1, 6, 4, 7, base_gray());
        }

        if g[north].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 4, 8, 6, 4, 14, base_gray());
        }

        self.generate_box(
            level,
            chunk_bb,
            0,
            3,
            0,
            0,
            3,
            15,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            3,
            0,
            7,
            3,
            15,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            0,
            7,
            3,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            15,
            6,
            3,
            15,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            2,
            0,
            0,
            2,
            15,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            2,
            0,
            7,
            2,
            15,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            2,
            0,
            7,
            2,
            0,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            2,
            15,
            6,
            2,
            15,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            0,
            0,
            1,
            15,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            1,
            0,
            7,
            1,
            15,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            0,
            7,
            1,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            15,
            6,
            1,
            15,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            1,
            1,
            1,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            1,
            1,
            6,
            1,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            1,
            1,
            3,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            3,
            1,
            6,
            3,
            2,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            13,
            1,
            1,
            14,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            1,
            13,
            6,
            1,
            14,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            13,
            1,
            3,
            14,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            3,
            13,
            6,
            3,
            14,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            1,
            6,
            2,
            3,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            6,
            5,
            3,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            1,
            9,
            2,
            3,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            9,
            5,
            3,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            2,
            6,
            4,
            2,
            6,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            2,
            9,
            4,
            2,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            2,
            7,
            2,
            2,
            8,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            2,
            7,
            5,
            2,
            8,
            base_light(),
            base_light(),
            false,
        );
        self.place_block(level, lamp_block(), 2, 2, 5, chunk_bb);
        self.place_block(level, lamp_block(), 5, 2, 5, chunk_bb);
        self.place_block(level, lamp_block(), 2, 2, 10, chunk_bb);
        self.place_block(level, lamp_block(), 5, 2, 10, chunk_bb);
        self.place_block(level, base_light(), 2, 3, 5, chunk_bb);
        self.place_block(level, base_light(), 5, 3, 5, chunk_bb);
        self.place_block(level, base_light(), 2, 3, 10, chunk_bb);
        self.place_block(level, base_light(), 5, 3, 10, chunk_bb);
        if g[south].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 0, 4, 2, 0);
        }

        if g[south].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 7, 1, 3, 7, 2, 4);
        }

        if g[south].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 3, 0, 2, 4);
        }

        if g[north].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 15, 4, 2, 15);
        }

        if g[north].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 11, 0, 2, 12);
        }

        if g[north].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 7, 1, 11, 7, 2, 12);
        }
    }

    fn post_process_entry_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        self.generate_box(
            level,
            chunk_bb,
            0,
            3,
            0,
            2,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            3,
            0,
            7,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            2,
            0,
            1,
            2,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            2,
            0,
            7,
            2,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            0,
            0,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            1,
            0,
            7,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            7,
            7,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            0,
            2,
            3,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            5,
            1,
            0,
            6,
            3,
            0,
            base_light(),
            base_light(),
            false,
        );
        if g[room].has_opening[NORTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 7, 4, 2, 7);
        }

        if g[room].has_opening[WEST] {
            self.generate_water_box(level, chunk_bb, 0, 1, 3, 1, 2, 4);
        }

        if g[room].has_opening[EAST] {
            self.generate_water_box(level, chunk_bb, 6, 1, 3, 7, 2, 4);
        }
    }

    fn post_process_penthouse(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        self.generate_box(
            level,
            chunk_bb,
            2,
            -1,
            2,
            11,
            -1,
            11,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            -1,
            0,
            1,
            -1,
            11,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            12,
            -1,
            0,
            13,
            -1,
            11,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            -1,
            0,
            11,
            -1,
            1,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            2,
            -1,
            12,
            11,
            -1,
            13,
            base_gray(),
            base_gray(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            0,
            0,
            0,
            0,
            13,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            13,
            0,
            0,
            13,
            0,
            13,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            0,
            0,
            12,
            0,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            0,
            13,
            12,
            0,
            13,
            base_light(),
            base_light(),
            false,
        );

        for i in (2..=11).step_by(3) {
            self.place_block(level, lamp_block(), 0, 0, i, chunk_bb);
            self.place_block(level, lamp_block(), 13, 0, i, chunk_bb);
            self.place_block(level, lamp_block(), i, 0, 0, chunk_bb);
        }

        self.generate_box(
            level,
            chunk_bb,
            2,
            0,
            3,
            4,
            0,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            9,
            0,
            3,
            11,
            0,
            9,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            4,
            0,
            9,
            9,
            0,
            11,
            base_light(),
            base_light(),
            false,
        );
        self.place_block(level, base_light(), 5, 0, 8, chunk_bb);
        self.place_block(level, base_light(), 8, 0, 8, chunk_bb);
        self.place_block(level, base_light(), 10, 0, 10, chunk_bb);
        self.place_block(level, base_light(), 3, 0, 10, chunk_bb);
        self.generate_box(
            level,
            chunk_bb,
            3,
            0,
            3,
            3,
            0,
            7,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            0,
            3,
            10,
            0,
            7,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            0,
            10,
            7,
            0,
            10,
            base_black(),
            base_black(),
            false,
        );
        let mut x = 3;

        for i in 0..2 {
            for z in (2..=8).step_by(3) {
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    0,
                    z,
                    x,
                    2,
                    z,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            x = 10;
        }

        self.generate_box(
            level,
            chunk_bb,
            5,
            0,
            10,
            5,
            2,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            8,
            0,
            10,
            8,
            2,
            10,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            6,
            -1,
            7,
            7,
            -1,
            8,
            base_black(),
            base_black(),
            false,
        );
        self.generate_water_box(level, chunk_bb, 6, -1, 3, 7, -1, 4);
        self.spawn_elder(level, chunk_bb, 6, 1, 6);
    }

    fn post_process_simple_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 0, 0, g[room].has_opening[DOWN]);
        }

        if g[room].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 4, 1, 6, 4, 6, base_gray());
        }

        let center_pillar = self.main_design != 0
            && random.next_boolean()
            && !g[room].has_opening[DOWN]
            && !g[room].has_opening[UP]
            && g[room].count_openings() > 1;
        if self.main_design == 0 {
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                0,
                2,
                1,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                3,
                0,
                2,
                3,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                2,
                0,
                0,
                2,
                2,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                2,
                0,
                2,
                2,
                0,
                base_gray(),
                base_gray(),
                false,
            );
            self.place_block(level, lamp_block(), 1, 2, 1, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                5,
                1,
                0,
                7,
                1,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                3,
                0,
                7,
                3,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                2,
                0,
                7,
                2,
                2,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                2,
                0,
                6,
                2,
                0,
                base_gray(),
                base_gray(),
                false,
            );
            self.place_block(level, lamp_block(), 6, 2, 1, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                5,
                2,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                3,
                5,
                2,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                2,
                5,
                0,
                2,
                7,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                2,
                7,
                2,
                2,
                7,
                base_gray(),
                base_gray(),
                false,
            );
            self.place_block(level, lamp_block(), 1, 2, 6, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                5,
                1,
                5,
                7,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                3,
                5,
                7,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                2,
                5,
                7,
                2,
                7,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                2,
                7,
                6,
                2,
                7,
                base_gray(),
                base_gray(),
                false,
            );
            self.place_block(level, lamp_block(), 6, 2, 6, chunk_bb);
            if g[room].has_opening[SOUTH] {
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    3,
                    0,
                    4,
                    3,
                    0,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    3,
                    0,
                    4,
                    3,
                    1,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    2,
                    0,
                    4,
                    2,
                    0,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    1,
                    0,
                    4,
                    1,
                    1,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            if g[room].has_opening[NORTH] {
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    3,
                    7,
                    4,
                    3,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    3,
                    6,
                    4,
                    3,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    2,
                    7,
                    4,
                    2,
                    7,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    3,
                    1,
                    6,
                    4,
                    1,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            if g[room].has_opening[WEST] {
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    3,
                    3,
                    0,
                    3,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    3,
                    3,
                    1,
                    3,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    2,
                    3,
                    0,
                    2,
                    4,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    1,
                    3,
                    1,
                    1,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            if g[room].has_opening[EAST] {
                self.generate_box(
                    level,
                    chunk_bb,
                    7,
                    3,
                    3,
                    7,
                    3,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
            } else {
                self.generate_box(
                    level,
                    chunk_bb,
                    6,
                    3,
                    3,
                    7,
                    3,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    7,
                    2,
                    3,
                    7,
                    2,
                    4,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    6,
                    1,
                    3,
                    7,
                    1,
                    4,
                    base_light(),
                    base_light(),
                    false,
                );
            }
        } else if self.main_design == 1 {
            self.generate_box(
                level,
                chunk_bb,
                2,
                1,
                2,
                2,
                3,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                2,
                1,
                5,
                2,
                3,
                5,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                1,
                5,
                5,
                3,
                5,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                1,
                2,
                5,
                3,
                2,
                base_light(),
                base_light(),
                false,
            );
            self.place_block(level, lamp_block(), 2, 2, 2, chunk_bb);
            self.place_block(level, lamp_block(), 2, 2, 5, chunk_bb);
            self.place_block(level, lamp_block(), 5, 2, 5, chunk_bb);
            self.place_block(level, lamp_block(), 5, 2, 2, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                0,
                1,
                3,
                0,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                1,
                0,
                3,
                1,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                7,
                1,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                6,
                0,
                3,
                6,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                1,
                7,
                7,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                1,
                6,
                7,
                3,
                6,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                1,
                0,
                7,
                3,
                0,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                1,
                1,
                7,
                3,
                1,
                base_light(),
                base_light(),
                false,
            );
            self.place_block(level, base_gray(), 1, 2, 0, chunk_bb);
            self.place_block(level, base_gray(), 0, 2, 1, chunk_bb);
            self.place_block(level, base_gray(), 1, 2, 7, chunk_bb);
            self.place_block(level, base_gray(), 0, 2, 6, chunk_bb);
            self.place_block(level, base_gray(), 6, 2, 7, chunk_bb);
            self.place_block(level, base_gray(), 7, 2, 6, chunk_bb);
            self.place_block(level, base_gray(), 6, 2, 0, chunk_bb);
            self.place_block(level, base_gray(), 7, 2, 1, chunk_bb);
            if !g[room].has_opening[SOUTH] {
                self.generate_box(
                    level,
                    chunk_bb,
                    1,
                    3,
                    0,
                    6,
                    3,
                    0,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    1,
                    2,
                    0,
                    6,
                    2,
                    0,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    1,
                    1,
                    0,
                    6,
                    1,
                    0,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            if !g[room].has_opening[NORTH] {
                self.generate_box(
                    level,
                    chunk_bb,
                    1,
                    3,
                    7,
                    6,
                    3,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    1,
                    2,
                    7,
                    6,
                    2,
                    7,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    1,
                    1,
                    7,
                    6,
                    1,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            if !g[room].has_opening[WEST] {
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    3,
                    1,
                    0,
                    3,
                    6,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    2,
                    1,
                    0,
                    2,
                    6,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    0,
                    1,
                    1,
                    0,
                    1,
                    6,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            if !g[room].has_opening[EAST] {
                self.generate_box(
                    level,
                    chunk_bb,
                    7,
                    3,
                    1,
                    7,
                    3,
                    6,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    7,
                    2,
                    1,
                    7,
                    2,
                    6,
                    base_gray(),
                    base_gray(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    7,
                    1,
                    1,
                    7,
                    1,
                    6,
                    base_light(),
                    base_light(),
                    false,
                );
            }
        } else if self.main_design == 2 {
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                0,
                0,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                1,
                0,
                7,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                1,
                0,
                6,
                1,
                0,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                1,
                7,
                6,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                2,
                0,
                0,
                2,
                7,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                2,
                0,
                7,
                2,
                7,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                2,
                0,
                6,
                2,
                0,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                2,
                7,
                6,
                2,
                7,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                3,
                0,
                0,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                3,
                0,
                7,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                3,
                0,
                6,
                3,
                0,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                1,
                3,
                7,
                6,
                3,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                3,
                0,
                2,
                4,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                1,
                3,
                7,
                2,
                4,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                3,
                1,
                0,
                4,
                2,
                0,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                3,
                1,
                7,
                4,
                2,
                7,
                base_black(),
                base_black(),
                false,
            );
            if g[room].has_opening[SOUTH] {
                self.generate_water_box(level, chunk_bb, 3, 1, 0, 4, 2, 0);
            }

            if g[room].has_opening[NORTH] {
                self.generate_water_box(level, chunk_bb, 3, 1, 7, 4, 2, 7);
            }

            if g[room].has_opening[WEST] {
                self.generate_water_box(level, chunk_bb, 0, 1, 3, 0, 2, 4);
            }

            if g[room].has_opening[EAST] {
                self.generate_water_box(level, chunk_bb, 7, 1, 3, 7, 2, 4);
            }
        }

        if center_pillar {
            self.generate_box(
                level,
                chunk_bb,
                3,
                1,
                3,
                4,
                1,
                4,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                3,
                2,
                3,
                4,
                2,
                4,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                3,
                3,
                3,
                4,
                3,
                4,
                base_light(),
                base_light(),
                false,
            );
        }
    }

    fn post_process_simple_top_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        let (g, room) = self.room_graph();
        if g[room].index / 25 > 0 {
            self.generate_default_floor(level, chunk_bb, 0, 0, g[room].has_opening[DOWN]);
        }

        if g[room].connections[UP].is_none() {
            self.generate_box_on_fill_only(level, chunk_bb, 1, 4, 1, 6, 4, 6, base_gray());
        }

        for x in 1..=6 {
            for z in 1..=6 {
                if random.next_int_bounded(3) != 0 {
                    let y0 = 2 + if random.next_int_bounded(4) == 0 {
                        0
                    } else {
                        1
                    };
                    let mut wet_sponge = Block::WET_SPONGE.default_state();
                    self.generate_box(
                        level, chunk_bb, x, y0, z, x, 3, z, wet_sponge, wet_sponge, false,
                    );
                }
            }
        }

        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            0,
            0,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            1,
            0,
            7,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            0,
            6,
            1,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            7,
            6,
            1,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            2,
            0,
            0,
            2,
            7,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            2,
            0,
            7,
            2,
            7,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            2,
            0,
            6,
            2,
            0,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            2,
            7,
            6,
            2,
            7,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            3,
            0,
            0,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            3,
            0,
            7,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            0,
            6,
            3,
            0,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            1,
            3,
            7,
            6,
            3,
            7,
            base_light(),
            base_light(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            3,
            0,
            2,
            4,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            7,
            1,
            3,
            7,
            2,
            4,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            0,
            4,
            2,
            0,
            base_black(),
            base_black(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            7,
            4,
            2,
            7,
            base_black(),
            base_black(),
            false,
        );
        if g[room].has_opening[SOUTH] {
            self.generate_water_box(level, chunk_bb, 3, 1, 0, 4, 2, 0);
        }
    }

    fn post_process_wing_room(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.main_design == 0 {
            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    10 - i,
                    3 - i,
                    20 - i,
                    12 + i,
                    3 - i,
                    20,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            self.generate_box(
                level,
                chunk_bb,
                7,
                0,
                6,
                15,
                0,
                16,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                6,
                0,
                6,
                6,
                3,
                20,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                16,
                0,
                6,
                16,
                3,
                20,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                1,
                7,
                7,
                1,
                20,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                15,
                1,
                7,
                15,
                1,
                20,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                7,
                1,
                6,
                9,
                3,
                6,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                13,
                1,
                6,
                15,
                3,
                6,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                8,
                1,
                7,
                9,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                13,
                1,
                7,
                14,
                1,
                7,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                9,
                0,
                5,
                13,
                0,
                5,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                10,
                0,
                7,
                12,
                0,
                7,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                8,
                0,
                10,
                8,
                0,
                12,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                14,
                0,
                10,
                14,
                0,
                12,
                base_black(),
                base_black(),
                false,
            );

            for z in (7..=18).rev().step_by(3) {
                self.place_block(level, lamp_block(), 6, 3, z, chunk_bb);
                self.place_block(level, lamp_block(), 16, 3, z, chunk_bb);
            }

            self.place_block(level, lamp_block(), 10, 0, 10, chunk_bb);
            self.place_block(level, lamp_block(), 12, 0, 10, chunk_bb);
            self.place_block(level, lamp_block(), 10, 0, 12, chunk_bb);
            self.place_block(level, lamp_block(), 12, 0, 12, chunk_bb);
            self.place_block(level, lamp_block(), 8, 3, 6, chunk_bb);
            self.place_block(level, lamp_block(), 14, 3, 6, chunk_bb);
            self.place_block(level, base_light(), 4, 2, 4, chunk_bb);
            self.place_block(level, lamp_block(), 4, 1, 4, chunk_bb);
            self.place_block(level, base_light(), 4, 0, 4, chunk_bb);
            self.place_block(level, base_light(), 18, 2, 4, chunk_bb);
            self.place_block(level, lamp_block(), 18, 1, 4, chunk_bb);
            self.place_block(level, base_light(), 18, 0, 4, chunk_bb);
            self.place_block(level, base_light(), 4, 2, 18, chunk_bb);
            self.place_block(level, lamp_block(), 4, 1, 18, chunk_bb);
            self.place_block(level, base_light(), 4, 0, 18, chunk_bb);
            self.place_block(level, base_light(), 18, 2, 18, chunk_bb);
            self.place_block(level, lamp_block(), 18, 1, 18, chunk_bb);
            self.place_block(level, base_light(), 18, 0, 18, chunk_bb);
            self.place_block(level, base_light(), 9, 7, 20, chunk_bb);
            self.place_block(level, base_light(), 13, 7, 20, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                6,
                0,
                21,
                7,
                4,
                21,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                15,
                0,
                21,
                16,
                4,
                21,
                base_light(),
                base_light(),
                false,
            );
            self.spawn_elder(level, chunk_bb, 11, 2, 16);
        } else if self.main_design == 1 {
            self.generate_box(
                level,
                chunk_bb,
                9,
                3,
                18,
                13,
                3,
                20,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                9,
                0,
                18,
                9,
                2,
                18,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                13,
                0,
                18,
                13,
                2,
                18,
                base_light(),
                base_light(),
                false,
            );
            let mut x = 9;
            let mut z = 20;
            let mut y = 5;

            for i in 0..2 {
                self.place_block(level, base_light(), x, 6, 20, chunk_bb);
                self.place_block(level, lamp_block(), x, 5, 20, chunk_bb);
                self.place_block(level, base_light(), x, 4, 20, chunk_bb);
                x = 13;
            }

            self.generate_box(
                level,
                chunk_bb,
                7,
                3,
                7,
                15,
                3,
                14,
                base_light(),
                base_light(),
                false,
            );
            let mut var14 = 10;

            for i in 0..2 {
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    0,
                    10,
                    var14,
                    6,
                    10,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    0,
                    12,
                    var14,
                    6,
                    12,
                    base_light(),
                    base_light(),
                    false,
                );
                self.place_block(level, lamp_block(), var14, 0, 10, chunk_bb);
                self.place_block(level, lamp_block(), var14, 0, 12, chunk_bb);
                self.place_block(level, lamp_block(), var14, 4, 10, chunk_bb);
                self.place_block(level, lamp_block(), var14, 4, 12, chunk_bb);
                var14 = 12;
            }

            var14 = 8;

            for i in 0..2 {
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    0,
                    7,
                    var14,
                    2,
                    7,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    var14,
                    0,
                    14,
                    var14,
                    2,
                    14,
                    base_light(),
                    base_light(),
                    false,
                );
                var14 = 14;
            }

            self.generate_box(
                level,
                chunk_bb,
                8,
                3,
                8,
                8,
                3,
                13,
                base_black(),
                base_black(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                14,
                3,
                8,
                14,
                3,
                13,
                base_black(),
                base_black(),
                false,
            );
            self.spawn_elder(level, chunk_bb, 11, 5, 13);
        }
    }
}

/// `MonumentBuilding`.
pub struct MonumentBuilding {
    base: PieceBase,
    child_pieces: Vec<MonumentPiece>,
}

/// A room the fitters picked, before the (then final) room graph is shared with the piece.
struct PendingRoom {
    kind: PieceKind,
    room: usize,
    main_design: i32,
}

/// `MonumentRoomFitter.fits(definition)`.
fn fitter_fits(fitter: PieceKind, g: &RoomGraph, definition: usize) -> bool {
    let d = &g[definition];
    let open_unclaimed = |room: &RoomDefinition, direction: usize| {
        room.has_opening[direction]
            && !g[room.connections[direction].expect("room connection")].claimed
    };
    match fitter {
        PieceKind::DoubleX => open_unclaimed(d, EAST),
        PieceKind::DoubleXY => {
            if open_unclaimed(d, EAST) && open_unclaimed(d, UP) {
                open_unclaimed(&g[g.conn(definition, EAST)], UP)
            } else {
                false
            }
        }
        PieceKind::DoubleY => open_unclaimed(d, UP),
        PieceKind::DoubleYZ => {
            if open_unclaimed(d, NORTH) && open_unclaimed(d, UP) {
                open_unclaimed(&g[g.conn(definition, NORTH)], UP)
            } else {
                false
            }
        }
        PieceKind::DoubleZ => open_unclaimed(d, NORTH),
        PieceKind::Simple => true,
        PieceKind::SimpleTop => {
            !d.has_opening[WEST]
                && !d.has_opening[EAST]
                && !d.has_opening[NORTH]
                && !d.has_opening[SOUTH]
                && !d.has_opening[UP]
        }
        _ => unreachable!("not a fitter"),
    }
}

/// `MonumentRoomFitter.create(orientation, definition, random)`.
fn fitter_create(
    fitter: PieceKind,
    g: &mut RoomGraph,
    definition: usize,
    random: &mut dyn RandomSource,
) -> PendingRoom {
    let mut main_design = 0;
    let mut room = definition;
    match fitter {
        PieceKind::DoubleX => {
            g[definition].claimed = true;
            let east = g.conn(definition, EAST);
            g[east].claimed = true;
        }
        PieceKind::DoubleXY => {
            g[definition].claimed = true;
            let east = g.conn(definition, EAST);
            let up = g.conn(definition, UP);
            let east_up = g.conn(east, UP);
            g[east].claimed = true;
            g[up].claimed = true;
            g[east_up].claimed = true;
        }
        PieceKind::DoubleY => {
            g[definition].claimed = true;
            let up = g.conn(definition, UP);
            g[up].claimed = true;
        }
        PieceKind::DoubleYZ => {
            g[definition].claimed = true;
            let north = g.conn(definition, NORTH);
            let up = g.conn(definition, UP);
            let north_up = g.conn(north, UP);
            g[north].claimed = true;
            g[up].claimed = true;
            g[north_up].claimed = true;
        }
        PieceKind::DoubleZ => {
            if !g[definition].has_opening[NORTH] || g[g.conn(definition, NORTH)].claimed {
                room = g.conn(definition, SOUTH);
            }
            g[room].claimed = true;
            let north = g.conn(room, NORTH);
            g[north].claimed = true;
        }
        PieceKind::Simple => {
            g[definition].claimed = true;
            main_design = random.next_int_bounded(3);
        }
        PieceKind::SimpleTop => g[definition].claimed = true,
        _ => unreachable!("not a fitter"),
    }
    PendingRoom {
        kind: fitter,
        room,
        main_design,
    }
}

impl MonumentBuilding {
    /// `MonumentBuilding(random, west, north, direction)`.
    pub fn new(
        random: &mut dyn RandomSource,
        west: i32,
        north: i32,
        direction: Direction,
    ) -> MonumentBuilding {
        let mut base = PieceBase::new(
            &OMB,
            0,
            make_bounding_box(west, 39, north, direction, 58, 23, 58),
        );
        base.set_orientation(Some(direction));
        let mut building = MonumentBuilding {
            base,
            child_pieces: Vec::new(),
        };
        let (mut graph, source_room, core_room, room_definitions) =
            Self::generate_room_graph(random);
        graph[source_room].claimed = true;
        let mut pending = vec![
            PendingRoom {
                kind: PieceKind::Entry,
                room: source_room,
                main_design: 0,
            },
            PendingRoom {
                kind: PieceKind::Core,
                room: core_room,
                main_design: 0,
            },
        ];
        let fitters = [
            PieceKind::DoubleXY,
            PieceKind::DoubleYZ,
            PieceKind::DoubleZ,
            PieceKind::DoubleX,
            PieceKind::DoubleY,
            PieceKind::SimpleTop,
            PieceKind::Simple,
        ];
        for &definition in &room_definitions {
            if !graph[definition].claimed && !graph[definition].is_special() {
                for &fitter in &fitters {
                    if fitter_fits(fitter, &graph, definition) {
                        pending.push(fitter_create(fitter, &mut graph, definition, random));
                        break;
                    }
                }
            }
        }
        let graph = Arc::new(graph);
        let offset = building.get_world_pos(9, 0, 22);
        for room in pending {
            let mut child = MonumentPiece::new_room(
                room.kind,
                direction,
                graph.clone(),
                room.room,
                room.main_design,
            );
            child.base.bounding_box.move_vec(&offset);
            building.child_pieces.push(child);
        }
        let left_wing = BoundingBox::from_corners(
            &building.get_world_pos(1, 1, 1),
            &building.get_world_pos(23, 8, 21),
        );
        let right_wing = BoundingBox::from_corners(
            &building.get_world_pos(34, 1, 1),
            &building.get_world_pos(56, 8, 21),
        );
        let penthouse = BoundingBox::from_corners(
            &building.get_world_pos(22, 13, 22),
            &building.get_world_pos(35, 17, 35),
        );
        let wing_random = random.next_int();
        building.child_pieces.push(MonumentPiece::new_box(
            PieceKind::Wing,
            direction,
            left_wing,
            wing_random & 1,
        ));
        building.child_pieces.push(MonumentPiece::new_box(
            PieceKind::Wing,
            direction,
            right_wing,
            wing_random.wrapping_add(1) & 1,
        ));
        building.child_pieces.push(MonumentPiece::new_box(
            PieceKind::Penthouse,
            direction,
            penthouse,
            0,
        ));
        building
    }

    /// `MonumentBuilding(tag)`: no child pieces (see `OceanMonumentStructure.regeneratePiecesAfterLoad`).
    fn load(tag: &CompoundTag) -> Result<MonumentBuilding, String> {
        Ok(MonumentBuilding {
            base: PieceBase::load(&OMB, tag)?,
            child_pieces: Vec::new(),
        })
    }

    /// `generateRoomGraph(random)`: the graph, the source and core room ids and the shuffled room list (with the roof and
    /// wing rooms appended).
    fn generate_room_graph(random: &mut dyn RandomSource) -> (RoomGraph, usize, usize, Vec<usize>) {
        let mut graph = RoomGraph::default();
        let mut room_grid: [Option<usize>; 75] = [None; 75];
        for x in 0..5 {
            for z in 0..4 {
                let pos = get_room_index(x, 0, z);
                room_grid[pos as usize] = Some(graph.add(pos));
            }
        }
        for x in 0..5 {
            for z in 0..4 {
                let pos = get_room_index(x, 1, z);
                room_grid[pos as usize] = Some(graph.add(pos));
            }
        }
        for x in 1..4 {
            for z in 0..2 {
                let pos = get_room_index(x, 2, z);
                room_grid[pos as usize] = Some(graph.add(pos));
            }
        }
        let source_room = room_grid[GRIDROOM_SOURCE_INDEX as usize].expect("source room");
        for x in 0..5 {
            for z in 0..5 {
                for y in 0..3 {
                    let pos = get_room_index(x, y, z);
                    if let Some(room) = room_grid[pos as usize] {
                        for direction in Direction::VALUES {
                            let neigh_x = x + direction.get_step_x();
                            let neigh_y = y + direction.get_step_y();
                            let neigh_z = z + direction.get_step_z();
                            if (0..5).contains(&neigh_x)
                                && (0..5).contains(&neigh_z)
                                && (0..3).contains(&neigh_y)
                            {
                                let neigh_pos = get_room_index(neigh_x, neigh_y, neigh_z);
                                if let Some(neigh) = room_grid[neigh_pos as usize] {
                                    if neigh_z == z {
                                        graph.set_connection(room, direction, neigh);
                                    } else {
                                        graph.set_connection(room, direction.get_opposite(), neigh);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let roof_room = graph.add(1003);
        let left_wing = graph.add(1001);
        let right_wing = graph.add(1002);
        graph.set_connection(
            room_grid[GRIDROOM_TOP_CONNECT_INDEX as usize].expect("top connect room"),
            Direction::Up,
            roof_room,
        );
        graph.set_connection(
            room_grid[GRIDROOM_LEFTWING_CONNECT_INDEX as usize].expect("left wing connect room"),
            Direction::South,
            left_wing,
        );
        graph.set_connection(
            room_grid[GRIDROOM_RIGHTWING_CONNECT_INDEX as usize].expect("right wing connect room"),
            Direction::South,
            right_wing,
        );
        graph[roof_room].claimed = true;
        graph[left_wing].claimed = true;
        graph[right_wing].claimed = true;
        graph[source_room].is_source = true;
        let core_room = room_grid[get_room_index(random.next_int_bounded(4), 0, 2) as usize]
            .expect("core room");
        graph[core_room].claimed = true;
        let east = graph.conn(core_room, EAST);
        let north = graph.conn(core_room, NORTH);
        let up = graph.conn(core_room, UP);
        let east_north = graph.conn(east, NORTH);
        let east_up = graph.conn(east, UP);
        let north_up = graph.conn(north, UP);
        let east_north_up = graph.conn(east_north, UP);
        for room in [
            east,
            north,
            east_north,
            up,
            east_up,
            north_up,
            east_north_up,
        ] {
            graph[room].claimed = true;
        }
        let mut room_defs = Vec::new();
        for room in room_grid.into_iter().flatten() {
            graph.update_openings(room);
            room_defs.push(room);
        }
        graph.update_openings(roof_room);
        shuffle(&mut room_defs, random);
        let mut scan_index = 1;
        for &definition in &room_defs {
            let mut close_count = 0;
            let mut attempt_count = 0;
            while close_count < 2 && attempt_count < 5 {
                attempt_count += 1;
                let f = random.next_int_bounded(6) as usize;
                if graph[definition].has_opening[f] {
                    let of = Direction::from_3d_data_value(f as i32)
                        .get_opposite()
                        .get_3d_data_value();
                    graph[definition].has_opening[f] = false;
                    let other = graph[definition].connections[f].expect("room connection");
                    graph[other].has_opening[of] = false;
                    let found = graph.find_source(definition, scan_index);
                    scan_index += 1;
                    if found && {
                        let found_other = graph.find_source(other, scan_index);
                        scan_index += 1;
                        found_other
                    } {
                        close_count += 1;
                    } else {
                        graph[definition].has_opening[f] = true;
                        graph[other].has_opening[of] = true;
                    }
                }
            }
        }
        room_defs.push(roof_room);
        room_defs.push(left_wing);
        room_defs.push(right_wing);
        (graph, source_room, core_room, room_defs)
    }
}

impl StructurePiece for MonumentBuilding {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        _tag: &mut CompoundTag,
    ) {
    }

    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        structure_manager: &StructureManager,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        chunk_pos: ChunkPos,
        reference_pos: BlockPos,
    ) {
        let water_height = level.get_sea_level().max(64) - self.base.bounding_box.min_y();
        self.generate_water_box(level, chunk_bb, 0, 0, 0, 58, water_height, 58);
        self.generate_wing(false, 0, level, random, chunk_bb);
        self.generate_wing(true, 33, level, random, chunk_bb);
        self.generate_entrance_archs(level, random, chunk_bb);
        self.generate_entrance_wall(level, random, chunk_bb);
        self.generate_roof_piece(level, random, chunk_bb);
        self.generate_lower_wall(level, random, chunk_bb);
        self.generate_middle_wall(level, random, chunk_bb);
        self.generate_upper_wall(level, random, chunk_bb);
        for pillar_x in 0..7 {
            let mut pillar_z = 0;
            while pillar_z < 7 {
                if pillar_z == 0 && pillar_x == 3 {
                    pillar_z = 6;
                }
                let bx = pillar_x * 9;
                let bz = pillar_z * 9;
                for w in 0..4 {
                    for d in 0..4 {
                        self.place_block(level, base_light(), bx + w, 0, bz + d, chunk_bb);
                        self.fill_column_down(level, base_light(), bx + w, -1, bz + d, chunk_bb);
                    }
                }
                if pillar_x != 0 && pillar_x != 6 {
                    pillar_z += 6;
                } else {
                    pillar_z += 1;
                }
            }
        }
        for i in 0..5 {
            self.generate_water_box(level, chunk_bb, -1 - i, i * 2, -1 - i, -1 - i, 23, 58 + i);
            self.generate_water_box(level, chunk_bb, 58 + i, i * 2, -1 - i, 58 + i, 23, 58 + i);
            self.generate_water_box(level, chunk_bb, -i, i * 2, -1 - i, 57 + i, 23, -1 - i);
            self.generate_water_box(level, chunk_bb, -i, i * 2, 58 + i, 57 + i, 23, 58 + i);
        }
        for child in &mut self.child_pieces {
            if child.base.bounding_box.intersects(chunk_bb) {
                child.post_process(
                    level,
                    structure_manager,
                    generator,
                    random,
                    chunk_bb,
                    chunk_pos,
                    reference_pos,
                );
            }
        }
    }
}

#[allow(unused_variables, unused_mut)]
impl MonumentBuilding {
    fn generate_wing(
        &self,
        is_flipped: bool,
        xoff: i32,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, xoff, 0, xoff + 23, 20) {
            self.generate_box(
                level,
                chunk_bb,
                xoff + 0,
                0,
                0,
                xoff + 24,
                0,
                20,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, xoff + 0, 1, 0, xoff + 24, 10, 20);

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    xoff + i,
                    i + 1,
                    i,
                    xoff + i,
                    i + 1,
                    20,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    xoff + i + 7,
                    i + 5,
                    i + 7,
                    xoff + i + 7,
                    i + 5,
                    20,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    xoff + 17 - i,
                    i + 5,
                    i + 7,
                    xoff + 17 - i,
                    i + 5,
                    20,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    xoff + 24 - i,
                    i + 1,
                    i,
                    xoff + 24 - i,
                    i + 1,
                    20,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    xoff + i + 1,
                    i + 1,
                    i,
                    xoff + 23 - i,
                    i + 1,
                    i,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    xoff + i + 8,
                    i + 5,
                    i + 7,
                    xoff + 16 - i,
                    i + 5,
                    i + 7,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            self.generate_box(
                level,
                chunk_bb,
                xoff + 4,
                4,
                4,
                xoff + 6,
                4,
                20,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                xoff + 7,
                4,
                4,
                xoff + 17,
                4,
                6,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                xoff + 18,
                4,
                4,
                xoff + 20,
                4,
                20,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                xoff + 11,
                8,
                11,
                xoff + 13,
                8,
                20,
                base_gray(),
                base_gray(),
                false,
            );
            self.place_block(level, dot_deco_data(), xoff + 12, 9, 12, chunk_bb);
            self.place_block(level, dot_deco_data(), xoff + 12, 9, 15, chunk_bb);
            self.place_block(level, dot_deco_data(), xoff + 12, 9, 18, chunk_bb);
            let left_pos = xoff + if is_flipped { 19 } else { 5 };
            let right_pos = xoff + if is_flipped { 5 } else { 19 };

            for z in (5..=20).rev().step_by(3) {
                self.place_block(level, dot_deco_data(), left_pos, 5, z, chunk_bb);
            }

            for z in (7..=19).rev().step_by(3) {
                self.place_block(level, dot_deco_data(), right_pos, 5, z, chunk_bb);
            }

            for i in 0..4 {
                let pos = if is_flipped {
                    xoff + 24 - (17 - i * 3)
                } else {
                    xoff + 17 - i * 3
                };
                self.place_block(level, dot_deco_data(), pos, 5, 5, chunk_bb);
            }

            self.place_block(level, dot_deco_data(), right_pos, 5, 5, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                xoff + 11,
                1,
                12,
                xoff + 13,
                7,
                12,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                xoff + 12,
                1,
                11,
                xoff + 12,
                7,
                13,
                base_gray(),
                base_gray(),
                false,
            );
        }
    }

    fn generate_entrance_archs(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, 22, 5, 35, 17) {
            self.generate_water_box(level, chunk_bb, 25, 0, 0, 32, 8, 20);

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    24,
                    2,
                    5 + i * 4,
                    24,
                    4,
                    5 + i * 4,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    22,
                    4,
                    5 + i * 4,
                    23,
                    4,
                    5 + i * 4,
                    base_light(),
                    base_light(),
                    false,
                );
                self.place_block(level, base_light(), 25, 5, 5 + i * 4, chunk_bb);
                self.place_block(level, base_light(), 26, 6, 5 + i * 4, chunk_bb);
                self.place_block(level, lamp_block(), 26, 5, 5 + i * 4, chunk_bb);
                self.generate_box(
                    level,
                    chunk_bb,
                    33,
                    2,
                    5 + i * 4,
                    33,
                    4,
                    5 + i * 4,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    34,
                    4,
                    5 + i * 4,
                    35,
                    4,
                    5 + i * 4,
                    base_light(),
                    base_light(),
                    false,
                );
                self.place_block(level, base_light(), 32, 5, 5 + i * 4, chunk_bb);
                self.place_block(level, base_light(), 31, 6, 5 + i * 4, chunk_bb);
                self.place_block(level, lamp_block(), 31, 5, 5 + i * 4, chunk_bb);
                self.generate_box(
                    level,
                    chunk_bb,
                    27,
                    6,
                    5 + i * 4,
                    30,
                    6,
                    5 + i * 4,
                    base_gray(),
                    base_gray(),
                    false,
                );
            }
        }
    }

    fn generate_entrance_wall(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, 15, 20, 42, 21) {
            self.generate_box(
                level,
                chunk_bb,
                15,
                0,
                21,
                42,
                0,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 26, 1, 21, 31, 3, 21);
            self.generate_box(
                level,
                chunk_bb,
                21,
                12,
                21,
                36,
                12,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                17,
                11,
                21,
                40,
                11,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                16,
                10,
                21,
                41,
                10,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                15,
                7,
                21,
                42,
                9,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                16,
                6,
                21,
                41,
                6,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                17,
                5,
                21,
                40,
                5,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                21,
                4,
                21,
                36,
                4,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                22,
                3,
                21,
                26,
                3,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                31,
                3,
                21,
                35,
                3,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                23,
                2,
                21,
                25,
                2,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                32,
                2,
                21,
                34,
                2,
                21,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                28,
                4,
                20,
                29,
                4,
                21,
                base_light(),
                base_light(),
                false,
            );
            self.place_block(level, base_light(), 27, 3, 21, chunk_bb);
            self.place_block(level, base_light(), 30, 3, 21, chunk_bb);
            self.place_block(level, base_light(), 26, 2, 21, chunk_bb);
            self.place_block(level, base_light(), 31, 2, 21, chunk_bb);
            self.place_block(level, base_light(), 25, 1, 21, chunk_bb);
            self.place_block(level, base_light(), 32, 1, 21, chunk_bb);

            for i in 0..7 {
                self.place_block(level, base_black(), 28 - i, 6 + i, 21, chunk_bb);
                self.place_block(level, base_black(), 29 + i, 6 + i, 21, chunk_bb);
            }

            for i in 0..4 {
                self.place_block(level, base_black(), 28 - i, 9 + i, 21, chunk_bb);
                self.place_block(level, base_black(), 29 + i, 9 + i, 21, chunk_bb);
            }

            self.place_block(level, base_black(), 28, 12, 21, chunk_bb);
            self.place_block(level, base_black(), 29, 12, 21, chunk_bb);

            for i in 0..3 {
                self.place_block(level, base_black(), 22 - i * 2, 8, 21, chunk_bb);
                self.place_block(level, base_black(), 22 - i * 2, 9, 21, chunk_bb);
                self.place_block(level, base_black(), 35 + i * 2, 8, 21, chunk_bb);
                self.place_block(level, base_black(), 35 + i * 2, 9, 21, chunk_bb);
            }

            self.generate_water_box(level, chunk_bb, 15, 13, 21, 42, 15, 21);
            self.generate_water_box(level, chunk_bb, 15, 1, 21, 15, 6, 21);
            self.generate_water_box(level, chunk_bb, 16, 1, 21, 16, 5, 21);
            self.generate_water_box(level, chunk_bb, 17, 1, 21, 20, 4, 21);
            self.generate_water_box(level, chunk_bb, 21, 1, 21, 21, 3, 21);
            self.generate_water_box(level, chunk_bb, 22, 1, 21, 22, 2, 21);
            self.generate_water_box(level, chunk_bb, 23, 1, 21, 24, 1, 21);
            self.generate_water_box(level, chunk_bb, 42, 1, 21, 42, 6, 21);
            self.generate_water_box(level, chunk_bb, 41, 1, 21, 41, 5, 21);
            self.generate_water_box(level, chunk_bb, 37, 1, 21, 40, 4, 21);
            self.generate_water_box(level, chunk_bb, 36, 1, 21, 36, 3, 21);
            self.generate_water_box(level, chunk_bb, 33, 1, 21, 34, 1, 21);
            self.generate_water_box(level, chunk_bb, 35, 1, 21, 35, 2, 21);
        }
    }

    fn generate_roof_piece(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, 21, 21, 36, 36) {
            self.generate_box(
                level,
                chunk_bb,
                21,
                0,
                22,
                36,
                0,
                36,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 21, 1, 22, 36, 23, 36);

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    21 + i,
                    13 + i,
                    21 + i,
                    36 - i,
                    13 + i,
                    21 + i,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    21 + i,
                    13 + i,
                    36 - i,
                    36 - i,
                    13 + i,
                    36 - i,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    21 + i,
                    13 + i,
                    22 + i,
                    21 + i,
                    13 + i,
                    35 - i,
                    base_light(),
                    base_light(),
                    false,
                );
                self.generate_box(
                    level,
                    chunk_bb,
                    36 - i,
                    13 + i,
                    22 + i,
                    36 - i,
                    13 + i,
                    35 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            self.generate_box(
                level,
                chunk_bb,
                25,
                16,
                25,
                32,
                16,
                32,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                25,
                17,
                25,
                25,
                19,
                25,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                32,
                17,
                25,
                32,
                19,
                25,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                25,
                17,
                32,
                25,
                19,
                32,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                32,
                17,
                32,
                32,
                19,
                32,
                base_light(),
                base_light(),
                false,
            );
            self.place_block(level, base_light(), 26, 20, 26, chunk_bb);
            self.place_block(level, base_light(), 27, 21, 27, chunk_bb);
            self.place_block(level, lamp_block(), 27, 20, 27, chunk_bb);
            self.place_block(level, base_light(), 26, 20, 31, chunk_bb);
            self.place_block(level, base_light(), 27, 21, 30, chunk_bb);
            self.place_block(level, lamp_block(), 27, 20, 30, chunk_bb);
            self.place_block(level, base_light(), 31, 20, 31, chunk_bb);
            self.place_block(level, base_light(), 30, 21, 30, chunk_bb);
            self.place_block(level, lamp_block(), 30, 20, 30, chunk_bb);
            self.place_block(level, base_light(), 31, 20, 26, chunk_bb);
            self.place_block(level, base_light(), 30, 21, 27, chunk_bb);
            self.place_block(level, lamp_block(), 30, 20, 27, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                28,
                21,
                27,
                29,
                21,
                27,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                27,
                21,
                28,
                27,
                21,
                29,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                28,
                21,
                30,
                29,
                21,
                30,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                30,
                21,
                28,
                30,
                21,
                29,
                base_gray(),
                base_gray(),
                false,
            );
        }
    }

    fn generate_lower_wall(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, 0, 21, 6, 58) {
            self.generate_box(
                level,
                chunk_bb,
                0,
                0,
                21,
                6,
                0,
                57,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 0, 1, 21, 6, 7, 57);
            self.generate_box(
                level,
                chunk_bb,
                4,
                4,
                21,
                6,
                4,
                53,
                base_gray(),
                base_gray(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    i,
                    i + 1,
                    21,
                    i,
                    i + 1,
                    57 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for z in (23..53).step_by(3) {
                self.place_block(level, dot_deco_data(), 5, 5, z, chunk_bb);
            }

            self.place_block(level, dot_deco_data(), 5, 5, 52, chunk_bb);

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    i,
                    i + 1,
                    21,
                    i,
                    i + 1,
                    57 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            self.generate_box(
                level,
                chunk_bb,
                4,
                1,
                52,
                6,
                3,
                52,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                5,
                1,
                51,
                5,
                3,
                53,
                base_gray(),
                base_gray(),
                false,
            );
        }

        if self.chunk_intersects(chunk_bb, 51, 21, 58, 58) {
            self.generate_box(
                level,
                chunk_bb,
                51,
                0,
                21,
                57,
                0,
                57,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 51, 1, 21, 57, 7, 57);
            self.generate_box(
                level,
                chunk_bb,
                51,
                4,
                21,
                53,
                4,
                53,
                base_gray(),
                base_gray(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    57 - i,
                    i + 1,
                    21,
                    57 - i,
                    i + 1,
                    57 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for z in (23..53).step_by(3) {
                self.place_block(level, dot_deco_data(), 52, 5, z, chunk_bb);
            }

            self.place_block(level, dot_deco_data(), 52, 5, 52, chunk_bb);
            self.generate_box(
                level,
                chunk_bb,
                51,
                1,
                52,
                53,
                3,
                52,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                52,
                1,
                51,
                52,
                3,
                53,
                base_gray(),
                base_gray(),
                false,
            );
        }

        if self.chunk_intersects(chunk_bb, 0, 51, 57, 57) {
            self.generate_box(
                level,
                chunk_bb,
                7,
                0,
                51,
                50,
                0,
                57,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 7, 1, 51, 50, 10, 57);

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    i + 1,
                    i + 1,
                    57 - i,
                    56 - i,
                    i + 1,
                    57 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }
        }
    }

    fn generate_middle_wall(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, 7, 21, 13, 50) {
            self.generate_box(
                level,
                chunk_bb,
                7,
                0,
                21,
                13,
                0,
                50,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 7, 1, 21, 13, 10, 50);
            self.generate_box(
                level,
                chunk_bb,
                11,
                8,
                21,
                13,
                8,
                53,
                base_gray(),
                base_gray(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    i + 7,
                    i + 5,
                    21,
                    i + 7,
                    i + 5,
                    54,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for z in (21..=45).step_by(3) {
                self.place_block(level, dot_deco_data(), 12, 9, z, chunk_bb);
            }
        }

        if self.chunk_intersects(chunk_bb, 44, 21, 50, 54) {
            self.generate_box(
                level,
                chunk_bb,
                44,
                0,
                21,
                50,
                0,
                50,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 44, 1, 21, 50, 10, 50);
            self.generate_box(
                level,
                chunk_bb,
                44,
                8,
                21,
                46,
                8,
                53,
                base_gray(),
                base_gray(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    50 - i,
                    i + 5,
                    21,
                    50 - i,
                    i + 5,
                    54,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for z in (21..=45).step_by(3) {
                self.place_block(level, dot_deco_data(), 45, 9, z, chunk_bb);
            }
        }

        if self.chunk_intersects(chunk_bb, 8, 44, 49, 54) {
            self.generate_box(
                level,
                chunk_bb,
                14,
                0,
                44,
                43,
                0,
                50,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 14, 1, 44, 43, 10, 50);

            for x in (12..=45).step_by(3) {
                self.place_block(level, dot_deco_data(), x, 9, 45, chunk_bb);
                self.place_block(level, dot_deco_data(), x, 9, 52, chunk_bb);
                if x == 12 || x == 18 || x == 24 || x == 33 || x == 39 || x == 45 {
                    self.place_block(level, dot_deco_data(), x, 9, 47, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 9, 50, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 10, 45, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 10, 46, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 10, 51, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 10, 52, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 11, 47, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 11, 50, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 12, 48, chunk_bb);
                    self.place_block(level, dot_deco_data(), x, 12, 49, chunk_bb);
                }
            }

            for i in 0..3 {
                self.generate_box(
                    level,
                    chunk_bb,
                    8 + i,
                    5 + i,
                    54,
                    49 - i,
                    5 + i,
                    54,
                    base_gray(),
                    base_gray(),
                    false,
                );
            }

            self.generate_box(
                level,
                chunk_bb,
                11,
                8,
                54,
                46,
                8,
                54,
                base_light(),
                base_light(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                14,
                8,
                44,
                43,
                8,
                53,
                base_gray(),
                base_gray(),
                false,
            );
        }
    }

    fn generate_upper_wall(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        if self.chunk_intersects(chunk_bb, 14, 21, 20, 43) {
            self.generate_box(
                level,
                chunk_bb,
                14,
                0,
                21,
                20,
                0,
                43,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 14, 1, 22, 20, 14, 43);
            self.generate_box(
                level,
                chunk_bb,
                18,
                12,
                22,
                20,
                12,
                39,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                18,
                12,
                21,
                20,
                12,
                21,
                base_light(),
                base_light(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    i + 14,
                    i + 9,
                    21,
                    i + 14,
                    i + 9,
                    43 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for z in (23..=39).step_by(3) {
                self.place_block(level, dot_deco_data(), 19, 13, z, chunk_bb);
            }
        }

        if self.chunk_intersects(chunk_bb, 37, 21, 43, 43) {
            self.generate_box(
                level,
                chunk_bb,
                37,
                0,
                21,
                43,
                0,
                43,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 37, 1, 22, 43, 14, 43);
            self.generate_box(
                level,
                chunk_bb,
                37,
                12,
                22,
                39,
                12,
                39,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_box(
                level,
                chunk_bb,
                37,
                12,
                21,
                39,
                12,
                21,
                base_light(),
                base_light(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    43 - i,
                    i + 9,
                    21,
                    43 - i,
                    i + 9,
                    43 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for z in (23..=39).step_by(3) {
                self.place_block(level, dot_deco_data(), 38, 13, z, chunk_bb);
            }
        }

        if self.chunk_intersects(chunk_bb, 15, 37, 42, 43) {
            self.generate_box(
                level,
                chunk_bb,
                21,
                0,
                37,
                36,
                0,
                43,
                base_gray(),
                base_gray(),
                false,
            );
            self.generate_water_box(level, chunk_bb, 21, 1, 37, 36, 14, 43);
            self.generate_box(
                level,
                chunk_bb,
                21,
                12,
                37,
                36,
                12,
                39,
                base_gray(),
                base_gray(),
                false,
            );

            for i in 0..4 {
                self.generate_box(
                    level,
                    chunk_bb,
                    15 + i,
                    i + 9,
                    43 - i,
                    42 - i,
                    i + 9,
                    43 - i,
                    base_light(),
                    base_light(),
                    false,
                );
            }

            for x in (21..=36).step_by(3) {
                self.place_block(level, dot_deco_data(), x, 13, 38, chunk_bb);
            }
        }
    }
}
