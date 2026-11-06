//! `StrongholdPieces`: the stronghold's piece classes, their weights and the pending-children generation.
//!
//! Vanilla keeps the generation state partly in statics (`currentPieces`, `imposedPiece`, `totalWeight`, the
//! `PieceWeight.placeCount`s, reset by `resetPieces()` right before each `StartPiece` is created) and partly in the
//! `StartPiece` (`previousPiece`, `portalRoomPiece`, `pendingChildren`). Both live in [`StrongholdGenState`], owned by
//! the [`StartPiece`]; pieces reach it through the `startPiece` argument of `addChildren`. `pendingChildren` holds the
//! builder indices of the pieces still to expand.

use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{DoubleBlockHalf, SlabType};
use mc_data::props;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::block_entity_data::spawner_tag;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    BlockSelector, PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece,
    StructurePieceSerializationContext, StructurePieceType, cave_air,
    get_random_horizontal_direction, make_bounding_box,
};
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;

const MAX_DEPTH: i32 = 50;
const LOWEST_Y_POSITION: i32 = 10;
/// `MAGIC_START_Y`: the start piece's floor before `moveBelowSeaLevel`.
pub const MAGIC_START_Y: i32 = 64;

const CORRIDOR_LOOT: &str = "minecraft:chests/stronghold_corridor";
const LIBRARY_LOOT: &str = "minecraft:chests/stronghold_library";
const CROSSING_LOOT: &str = "minecraft:chests/stronghold_crossing";

/// The piece classes `STRONGHOLD_PIECE_WEIGHTS` (and `imposedPiece`) refer to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieceClass {
    Straight,
    PrisonHall,
    LeftTurn,
    RightTurn,
    RoomCrossing,
    StraightStairsDown,
    StairsDown,
    FiveCrossing,
    ChestCorridor,
    Library,
    PortalRoom,
}

/// `StrongholdPieces.PieceWeight` (the `Library`/`PortalRoom` depth conditions are the anonymous `doPlace` overrides).
#[derive(Clone, Copy, Debug)]
struct PieceWeight {
    piece_class: PieceClass,
    weight: i32,
    place_count: i32,
    max_place_count: i32,
}

impl PieceWeight {
    fn do_place(&self, depth: i32) -> bool {
        let base = self.max_place_count == 0 || self.place_count < self.max_place_count;
        match self.piece_class {
            PieceClass::Library => base && depth > 4,
            PieceClass::PortalRoom => base && depth > 5,
            _ => base,
        }
    }

    fn is_valid(&self) -> bool {
        self.max_place_count == 0 || self.place_count < self.max_place_count
    }
}

/// `STRONGHOLD_PIECE_WEIGHTS`.
const STRONGHOLD_PIECE_WEIGHTS: [PieceWeight; 11] = [
    PieceWeight {
        piece_class: PieceClass::Straight,
        weight: 40,
        place_count: 0,
        max_place_count: 0,
    },
    PieceWeight {
        piece_class: PieceClass::PrisonHall,
        weight: 5,
        place_count: 0,
        max_place_count: 5,
    },
    PieceWeight {
        piece_class: PieceClass::LeftTurn,
        weight: 20,
        place_count: 0,
        max_place_count: 0,
    },
    PieceWeight {
        piece_class: PieceClass::RightTurn,
        weight: 20,
        place_count: 0,
        max_place_count: 0,
    },
    PieceWeight {
        piece_class: PieceClass::RoomCrossing,
        weight: 10,
        place_count: 0,
        max_place_count: 6,
    },
    PieceWeight {
        piece_class: PieceClass::StraightStairsDown,
        weight: 5,
        place_count: 0,
        max_place_count: 5,
    },
    PieceWeight {
        piece_class: PieceClass::StairsDown,
        weight: 5,
        place_count: 0,
        max_place_count: 5,
    },
    PieceWeight {
        piece_class: PieceClass::FiveCrossing,
        weight: 5,
        place_count: 0,
        max_place_count: 4,
    },
    PieceWeight {
        piece_class: PieceClass::ChestCorridor,
        weight: 5,
        place_count: 0,
        max_place_count: 4,
    },
    PieceWeight {
        piece_class: PieceClass::Library,
        weight: 10,
        place_count: 0,
        max_place_count: 2,
    },
    PieceWeight {
        piece_class: PieceClass::PortalRoom,
        weight: 20,
        place_count: 0,
        max_place_count: 1,
    },
];

/// The generation state of one stronghold attempt: vanilla's statics (reset per attempt by `resetPieces`) plus the
/// `StartPiece` fields `previousPiece`, `portalRoomPiece` and `pendingChildren`.
#[derive(Clone, Debug)]
pub struct StrongholdGenState {
    current_pieces: Vec<PieceWeight>,
    imposed_piece: Option<PieceClass>,
    total_weight: i32,
    previous_piece: Option<PieceClass>,
    /// `portalRoomPiece.getLocatorPosition()` (the portal room's box centre, kept in step with the start piece's moves).
    portal_room_locator: Option<BlockPos>,
    /// Builder indices of `pendingChildren`.
    pending_children: Vec<usize>,
    /// `startPiece.getBoundingBox().minX()/minZ()` (the start piece only moves vertically).
    start_min_x: i32,
    start_min_z: i32,
}

impl StrongholdGenState {
    /// `resetPieces()` for a start piece whose box begins at (`start_min_x`, `start_min_z`).
    fn new(start_min_x: i32, start_min_z: i32) -> StrongholdGenState {
        StrongholdGenState {
            current_pieces: STRONGHOLD_PIECE_WEIGHTS.to_vec(),
            imposed_piece: None,
            total_weight: 0,
            previous_piece: None,
            portal_room_locator: None,
            pending_children: Vec::new(),
            start_min_x,
            start_min_z,
        }
    }

    /// `updatePieceWeight()`.
    fn update_piece_weight(&mut self) -> bool {
        let mut has_any_pieces = false;
        self.total_weight = 0;
        for piece in &self.current_pieces {
            if piece.max_place_count > 0 && piece.place_count < piece.max_place_count {
                has_any_pieces = true;
            }
            self.total_weight = self.total_weight.wrapping_add(piece.weight);
        }
        has_any_pieces
    }

    /// `portalRoomPiece != null`.
    pub fn has_portal_room(&self) -> bool {
        self.portal_room_locator.is_some()
    }

    /// `pendingChildren.isEmpty()`.
    pub fn has_pending_children(&self) -> bool {
        !self.pending_children.is_empty()
    }

    /// `pendingChildren.remove(index)` (the builder index of the removed piece).
    pub fn remove_pending_child(&mut self, index: usize) -> usize {
        self.pending_children.remove(index)
    }

    /// `pendingChildren.size()`.
    pub fn pending_children_len(&self) -> usize {
        self.pending_children.len()
    }
}

/// The start piece's generation state (`(StartPiece) startPiece` cast of the pieces' `addChildren`).
fn gen_state(start_piece: Option<&mut dyn StructurePiece>) -> &mut StrongholdGenState {
    &mut start_piece
        .and_then(|piece| piece.downcast_mut::<StartPiece>())
        .expect("stronghold pieces are expanded with the StartPiece")
        .state
}

/// `findAndCreatePieceFactory(pieceClass, builder, random, footX, footY, footZ, direction, depth)`.
#[allow(clippy::too_many_arguments)]
fn find_and_create_piece_factory(
    piece_class: PieceClass,
    builder: &StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
) -> Option<Box<dyn StructurePiece>> {
    let (b, r, x, y, z, d, g) = (builder, random, foot_x, foot_y, foot_z, direction, depth);
    match piece_class {
        PieceClass::Straight => Straight::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::PrisonHall => PrisonHall::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::LeftTurn => LeftTurn::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::RightTurn => RightTurn::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::RoomCrossing => RoomCrossing::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::StraightStairsDown => StraightStairsDown::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::StairsDown => StairsDown::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::FiveCrossing => FiveCrossing::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::ChestCorridor => ChestCorridor::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::Library => Library::create_piece(b, r, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
        PieceClass::PortalRoom => PortalRoom::create_piece(b, x, y, z, d, g)
            .map(|p| Box::new(p) as Box<dyn StructurePiece>),
    }
}

/// `generatePieceFromSmallDoor(startPiece, builder, random, footX, footY, footZ, direction, depth)`.
#[allow(clippy::too_many_arguments)]
fn generate_piece_from_small_door(
    state: &mut StrongholdGenState,
    builder: &StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
) -> Option<Box<dyn StructurePiece>> {
    if !state.update_piece_weight() {
        return None;
    }
    if let Some(imposed) = state.imposed_piece {
        let piece = find_and_create_piece_factory(
            imposed, builder, random, foot_x, foot_y, foot_z, direction, depth,
        );
        state.imposed_piece = None;
        if piece.is_some() {
            return piece;
        }
    }

    let mut num_attempts = 0;
    while num_attempts < 5 {
        num_attempts += 1;
        let mut weight_selection = random.next_int_bounded(state.total_weight);
        // `for (piece : currentPieces)`: after a failed `createPiece` the loop goes on with the next piece (the weight
        // selection stays negative), as in vanilla.
        for i in 0..state.current_pieces.len() {
            let piece = state.current_pieces[i];
            weight_selection = weight_selection.wrapping_sub(piece.weight);
            if weight_selection < 0 {
                if !piece.do_place(depth) || Some(piece.piece_class) == state.previous_piece {
                    break;
                }
                let stronghold_piece = find_and_create_piece_factory(
                    piece.piece_class,
                    builder,
                    random,
                    foot_x,
                    foot_y,
                    foot_z,
                    direction,
                    depth,
                );
                if stronghold_piece.is_some() {
                    state.current_pieces[i].place_count += 1;
                    state.previous_piece = Some(piece.piece_class);
                    if !state.current_pieces[i].is_valid() {
                        state.current_pieces.remove(i);
                    }
                    return stronghold_piece;
                }
            }
        }
    }

    let bounding_box =
        FillerCorridor::find_piece_box(builder, random, foot_x, foot_y, foot_z, direction);
    match bounding_box {
        Some(bb) if bb.min_y() > 1 => Some(Box::new(FillerCorridor::new(depth, bb, direction))),
        _ => None,
    }
}

/// `generateAndAddPiece(startPiece, builder, random, footX, footY, footZ, direction, depth)`: adds the piece to the
/// builder and the start piece's pending list; returns whether a piece was created.
#[allow(clippy::too_many_arguments)]
fn generate_and_add_piece(
    state: &mut StrongholdGenState,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    if foot_x.wrapping_sub(state.start_min_x).wrapping_abs() <= 112
        && foot_z.wrapping_sub(state.start_min_z).wrapping_abs() <= 112
    {
        let new_piece = generate_piece_from_small_door(
            state,
            builder,
            random,
            foot_x,
            foot_y,
            foot_z,
            direction,
            depth + 1,
        );
        if let Some(new_piece) = new_piece {
            state.pending_children.push(builder.len());
            builder.add_piece(new_piece);
            return true;
        }
    }
    false
}

/// `StrongholdPiece.SmallDoorType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmallDoorType {
    Opening,
    WoodDoor,
    Grates,
    IronDoor,
}

impl SmallDoorType {
    /// `LEGACY_CODEC` encode (`Enum.name()`).
    fn name(self) -> &'static str {
        match self {
            SmallDoorType::Opening => "OPENING",
            SmallDoorType::WoodDoor => "WOOD_DOOR",
            SmallDoorType::Grates => "GRATES",
            SmallDoorType::IronDoor => "IRON_DOOR",
        }
    }

    /// `LEGACY_CODEC` decode (`valueOf`).
    fn value_of(name: &str) -> Option<SmallDoorType> {
        match name {
            "OPENING" => Some(SmallDoorType::Opening),
            "WOOD_DOOR" => Some(SmallDoorType::WoodDoor),
            "GRATES" => Some(SmallDoorType::Grates),
            "IRON_DOOR" => Some(SmallDoorType::IronDoor),
            _ => None,
        }
    }
}

/// The state of `StrongholdPiece`: the base piece and the entry door.
#[derive(Clone, Debug)]
struct StrongholdPieceData {
    base: PieceBase,
    entry_door: SmallDoorType,
}

impl StrongholdPieceData {
    fn new(
        piece_type: &'static StructurePieceType,
        gen_depth: i32,
        bounding_box: BoundingBox,
    ) -> StrongholdPieceData {
        StrongholdPieceData {
            base: PieceBase::new(piece_type, gen_depth, bounding_box),
            entry_door: SmallDoorType::Opening,
        }
    }

    /// `StrongholdPiece(type, direction...)` + `setOrientation(direction)`.
    fn oriented(
        piece_type: &'static StructurePieceType,
        gen_depth: i32,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> StrongholdPieceData {
        let mut data = StrongholdPieceData::new(piece_type, gen_depth, bounding_box);
        data.base.set_orientation(Some(direction));
        data
    }

    /// `StrongholdPiece(type, tag)`: `EntryDoor` is required.
    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<StrongholdPieceData, String> {
        let base = PieceBase::load(piece_type, tag)?;
        let entry_door = tag
            .get_string("EntryDoor")
            .and_then(SmallDoorType::value_of)
            .ok_or_else(|| "No value present".to_owned())?;
        Ok(StrongholdPieceData { base, entry_door })
    }

    /// `StrongholdPiece.addAdditionalSaveData`.
    fn save(&self, tag: &mut CompoundTag) {
        tag.put_string("EntryDoor", self.entry_door.name());
    }
}

/// `StrongholdPiece.randomSmallDoor(random)`.
fn random_small_door(random: &mut dyn RandomSource) -> SmallDoorType {
    match random.next_int_bounded(5) {
        2 => SmallDoorType::WoodDoor,
        3 => SmallDoorType::Grates,
        4 => SmallDoorType::IronDoor,
        _ => SmallDoorType::Opening,
    }
}

/// `StrongholdPiece.isOkBox(box)`.
fn is_ok_box(bounding_box: &BoundingBox) -> bool {
    bounding_box.min_y() > LOWEST_Y_POSITION
}

/// `StrongholdPiece`: the door and child generation helpers shared by all stronghold pieces.
trait StrongholdPiece: StructurePiece {
    /// `generateSmallDoor(level, random, chunkBB, doorType, footX, footY, footZ)` (the `random` is unused).
    #[allow(clippy::too_many_arguments)]
    fn generate_small_door(
        &self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        door_type: SmallDoorType,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
    ) {
        let (x, y, z) = (foot_x, foot_y, foot_z);
        let stone_bricks = Block::STONE_BRICKS.default_state();
        match door_type {
            SmallDoorType::Opening => {
                self.generate_box(
                    level,
                    chunk_bb,
                    x,
                    y,
                    z,
                    x + 3 - 1,
                    y + 3 - 1,
                    z,
                    cave_air(),
                    cave_air(),
                    false,
                );
            }
            SmallDoorType::WoodDoor => {
                self.place_block(level, stone_bricks, x, y, z, chunk_bb);
                self.place_block(level, stone_bricks, x, y + 1, z, chunk_bb);
                self.place_block(level, stone_bricks, x, y + 2, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 1, y + 2, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 2, y + 2, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 2, y + 1, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 2, y, z, chunk_bb);
                self.place_block(
                    level,
                    Block::OAK_DOOR.default_state(),
                    x + 1,
                    y,
                    z,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    Block::OAK_DOOR
                        .default_state()
                        .set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper),
                    x + 1,
                    y + 1,
                    z,
                    chunk_bb,
                );
            }
            SmallDoorType::Grates => {
                self.place_block(level, cave_air(), x + 1, y, z, chunk_bb);
                self.place_block(level, cave_air(), x + 1, y + 1, z, chunk_bb);
                let bars = Block::IRON_BARS.default_state();
                self.place_block(level, bars.set(props::WEST, true), x, y, z, chunk_bb);
                self.place_block(level, bars.set(props::WEST, true), x, y + 1, z, chunk_bb);
                let bars_we = bars.set(props::EAST, true).set(props::WEST, true);
                self.place_block(level, bars_we, x, y + 2, z, chunk_bb);
                self.place_block(level, bars_we, x + 1, y + 2, z, chunk_bb);
                self.place_block(level, bars_we, x + 2, y + 2, z, chunk_bb);
                self.place_block(
                    level,
                    bars.set(props::EAST, true),
                    x + 2,
                    y + 1,
                    z,
                    chunk_bb,
                );
                self.place_block(level, bars.set(props::EAST, true), x + 2, y, z, chunk_bb);
            }
            SmallDoorType::IronDoor => {
                self.place_block(level, stone_bricks, x, y, z, chunk_bb);
                self.place_block(level, stone_bricks, x, y + 1, z, chunk_bb);
                self.place_block(level, stone_bricks, x, y + 2, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 1, y + 2, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 2, y + 2, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 2, y + 1, z, chunk_bb);
                self.place_block(level, stone_bricks, x + 2, y, z, chunk_bb);
                self.place_block(
                    level,
                    Block::IRON_DOOR.default_state(),
                    x + 1,
                    y,
                    z,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    Block::IRON_DOOR
                        .default_state()
                        .set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper),
                    x + 1,
                    y + 1,
                    z,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    Block::STONE_BUTTON
                        .default_state()
                        .set(props::HORIZONTAL_FACING, Direction::North),
                    x + 2,
                    y + 1,
                    z + 1,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    Block::STONE_BUTTON
                        .default_state()
                        .set(props::HORIZONTAL_FACING, Direction::South),
                    x + 2,
                    y + 1,
                    z - 1,
                    chunk_bb,
                );
            }
        }
    }

    /// `generateSmallDoorChildForward(startPiece, builder, random, xOff, yOff)`.
    fn generate_small_door_child_forward(
        &self,
        state: &mut StrongholdGenState,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        x_off: i32,
        y_off: i32,
    ) {
        let base = self.base();
        let bb = base.bounding_box;
        let gen_depth = base.gen_depth;
        let (x, z, direction) = match base.orientation() {
            Some(Direction::North) => (bb.min_x() + x_off, bb.min_z() - 1, Direction::North),
            Some(Direction::South) => (bb.min_x() + x_off, bb.max_z() + 1, Direction::South),
            Some(Direction::West) => (bb.min_x() - 1, bb.min_z() + x_off, Direction::West),
            Some(Direction::East) => (bb.max_x() + 1, bb.min_z() + x_off, Direction::East),
            _ => return,
        };
        generate_and_add_piece(
            state,
            builder,
            random,
            x,
            bb.min_y() + y_off,
            z,
            direction,
            gen_depth,
        );
    }

    /// `generateSmallDoorChildLeft(startPiece, builder, random, yOff, zOff)`.
    fn generate_small_door_child_left(
        &self,
        state: &mut StrongholdGenState,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        y_off: i32,
        z_off: i32,
    ) {
        let base = self.base();
        let bb = base.bounding_box;
        let gen_depth = base.gen_depth;
        let (x, z, direction) = match base.orientation() {
            Some(Direction::North | Direction::South) => {
                (bb.min_x() - 1, bb.min_z() + z_off, Direction::West)
            }
            Some(Direction::West | Direction::East) => {
                (bb.min_x() + z_off, bb.min_z() - 1, Direction::North)
            }
            _ => return,
        };
        generate_and_add_piece(
            state,
            builder,
            random,
            x,
            bb.min_y() + y_off,
            z,
            direction,
            gen_depth,
        );
    }

    /// `generateSmallDoorChildRight(startPiece, builder, random, yOff, zOff)`.
    fn generate_small_door_child_right(
        &self,
        state: &mut StrongholdGenState,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        y_off: i32,
        z_off: i32,
    ) {
        let base = self.base();
        let bb = base.bounding_box;
        let gen_depth = base.gen_depth;
        let (x, z, direction) = match base.orientation() {
            Some(Direction::North | Direction::South) => {
                (bb.max_x() + 1, bb.min_z() + z_off, Direction::East)
            }
            Some(Direction::West | Direction::East) => {
                (bb.min_x() + z_off, bb.max_z() + 1, Direction::South)
            }
            _ => return,
        };
        generate_and_add_piece(
            state,
            builder,
            random,
            x,
            bb.min_y() + y_off,
            z,
            direction,
            gen_depth,
        );
    }
}

/// `StrongholdPieces.SmoothStoneSelector`.
#[derive(Default)]
struct SmoothStoneSelector {
    next: Option<BlockState>,
}

impl BlockSelector for SmoothStoneSelector {
    fn next(
        &mut self,
        random: &mut dyn RandomSource,
        _world_x: i32,
        _world_y: i32,
        _world_z: i32,
        is_edge: bool,
    ) {
        self.next = Some(if is_edge {
            let selection = random.next_float();
            if selection < 0.2 {
                Block::CRACKED_STONE_BRICKS.default_state()
            } else if selection < 0.5 {
                Block::MOSSY_STONE_BRICKS.default_state()
            } else if selection < 0.55 {
                Block::INFESTED_STONE_BRICKS.default_state()
            } else {
                Block::STONE_BRICKS.default_state()
            }
        } else {
            cave_air()
        });
    }

    fn get_next(&self) -> BlockState {
        self.next.unwrap_or_else(cave_air)
    }
}

/// Per-type boilerplate: every stronghold piece type gets the `StrongholdPiece` helpers.
macro_rules! stronghold_piece {
    ($ty:ty) => {
        impl StrongholdPiece for $ty {}
    };
}

macro_rules! base_accessors {
    () => {
        fn base(&self) -> &PieceBase {
            &self.sh.base
        }

        fn base_mut(&mut self) -> &mut PieceBase {
            &mut self.sh.base
        }
    };
}

/// Registers a piece type: `StructurePieceType.setPieceId(Class::new, id)`.
macro_rules! piece_type {
    ($name:ident, $id:literal, $load:expr) => {
        #[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
        pub static $name: StructurePieceType = StructurePieceType {
            id: $id,
            load: |_context, tag| {
                let load: fn(
                    &'static StructurePieceType,
                    &CompoundTag,
                ) -> Result<Box<dyn StructurePiece>, String> = $load;
                load(&$name, tag)
            },
        };
    };
}

piece_type!(STRONGHOLD_CHEST_CORRIDOR, "shcc", |t, tag| Ok(Box::new(
    ChestCorridor::load(t, tag)?
)));
piece_type!(STRONGHOLD_FILLER_CORRIDOR, "shfc", |t, tag| Ok(Box::new(
    FillerCorridor::load(t, tag)?
)));
piece_type!(STRONGHOLD_FIVE_CROSSING, "sh5c", |t, tag| Ok(Box::new(
    FiveCrossing::load(t, tag)?
)));
piece_type!(STRONGHOLD_LEFT_TURN, "shlt", |t, tag| Ok(Box::new(
    LeftTurn::load(t, tag)?
)));
piece_type!(STRONGHOLD_LIBRARY, "shli", |t, tag| Ok(Box::new(
    Library::load(t, tag)?
)));
piece_type!(STRONGHOLD_PORTAL_ROOM, "shpr", |t, tag| Ok(Box::new(
    PortalRoom::load(t, tag)?
)));
piece_type!(STRONGHOLD_PRISON_HALL, "shph", |t, tag| Ok(Box::new(
    PrisonHall::load(t, tag)?
)));
piece_type!(STRONGHOLD_RIGHT_TURN, "shrt", |t, tag| Ok(Box::new(
    RightTurn::load(t, tag)?
)));
piece_type!(STRONGHOLD_ROOM_CROSSING, "shrc", |t, tag| Ok(Box::new(
    RoomCrossing::load(t, tag)?
)));
piece_type!(STRONGHOLD_STAIRS_DOWN, "shsd", |t, tag| Ok(Box::new(
    StairsDown::load(t, tag)?
)));
piece_type!(STRONGHOLD_START, "shstart", |t, tag| Ok(Box::new(
    StartPiece::load(t, tag)?
)));
piece_type!(STRONGHOLD_STRAIGHT, "shs", |t, tag| Ok(Box::new(
    Straight::load(t, tag)?
)));
piece_type!(STRONGHOLD_STRAIGHT_STAIRS_DOWN, "shssd", |t, tag| Ok(
    Box::new(StraightStairsDown::load(t, tag)?)
));

// ---------------------------------------------------------------------------------------------------------------------

/// `StrongholdPieces.ChestCorridor`.
pub struct ChestCorridor {
    sh: StrongholdPieceData,
    has_placed_chest: bool,
}

stronghold_piece!(ChestCorridor);

impl ChestCorridor {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> ChestCorridor {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_CHEST_CORRIDOR,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        ChestCorridor {
            sh,
            has_placed_chest: false,
        }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<ChestCorridor, String> {
        Ok(ChestCorridor {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            has_placed_chest: tag.get_boolean_or("Chest", false),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<ChestCorridor> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -1, 0, 5, 5, 7, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| ChestCorridor::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for ChestCorridor {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_boolean("Chest", self.has_placed_chest);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        self.generate_small_door_child_forward(gen_state(start_piece), builder, random, 1, 1);
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
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            4,
            4,
            6,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 1, 0);
        self.generate_small_door(level, chunk_bb, SmallDoorType::Opening, 1, 1, 6);
        let stone_bricks = Block::STONE_BRICKS.default_state();
        self.generate_box(
            level,
            chunk_bb,
            3,
            1,
            2,
            3,
            1,
            4,
            stone_bricks,
            stone_bricks,
            false,
        );
        let slab = Block::STONE_BRICK_SLAB.default_state();
        self.place_block(level, slab, 3, 1, 1, chunk_bb);
        self.place_block(level, slab, 3, 1, 5, chunk_bb);
        self.place_block(level, slab, 3, 2, 2, chunk_bb);
        self.place_block(level, slab, 3, 2, 4, chunk_bb);
        for z in 2..=4 {
            self.place_block(level, slab, 2, 1, z, chunk_bb);
        }
        if !self.has_placed_chest && chunk_bb.is_inside(&self.get_world_pos(3, 2, 3)) {
            self.has_placed_chest = true;
            self.create_chest(level, chunk_bb, random, 3, 2, 3, CORRIDOR_LOOT);
        }
    }
}

/// `StrongholdPieces.FillerCorridor`.
pub struct FillerCorridor {
    sh: StrongholdPieceData,
    steps: i32,
}

stronghold_piece!(FillerCorridor);

impl FillerCorridor {
    fn new(gen_depth: i32, bounding_box: BoundingBox, direction: Direction) -> FillerCorridor {
        let sh = StrongholdPieceData::oriented(
            &STRONGHOLD_FILLER_CORRIDOR,
            gen_depth,
            bounding_box,
            direction,
        );
        let steps = if direction != Direction::North && direction != Direction::South {
            bounding_box.get_x_span()
        } else {
            bounding_box.get_z_span()
        };
        FillerCorridor { sh, steps }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<FillerCorridor, String> {
        Ok(FillerCorridor {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            steps: tag.get_int_or("Steps", 0),
        })
    }

    /// `findPieceBox(builder, random, footX, footY, footZ, direction)` (the `random` is unused).
    fn find_piece_box(
        builder: &StructurePiecesBuilder,
        _random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
    ) -> Option<BoundingBox> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -1, 0, 5, 5, 4, direction);
        let collision_piece = builder.find_collision_piece(&bb)?;
        let collision_box = collision_piece.get_bounding_box();
        if collision_box.min_y() == bb.min_y() {
            for depth in (1..=2).rev() {
                let bb = BoundingBox::orient_box(
                    foot_x, foot_y, foot_z, -1, -1, 0, 5, 5, depth, direction,
                );
                if !collision_box.intersects(&bb) {
                    return Some(BoundingBox::orient_box(
                        foot_x,
                        foot_y,
                        foot_z,
                        -1,
                        -1,
                        0,
                        5,
                        5,
                        depth + 1,
                        direction,
                    ));
                }
            }
        }
        None
    }
}

impl StructurePiece for FillerCorridor {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_int("Steps", self.steps);
    }

    fn post_process(
        &mut self,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        _reference_pos: BlockPos,
    ) {
        let stone_bricks = Block::STONE_BRICKS.default_state();
        for i in 0..self.steps {
            for x in 0..=4 {
                self.place_block(level, stone_bricks, x, 0, i, chunk_bb);
            }
            for y in 1..=3 {
                self.place_block(level, stone_bricks, 0, y, i, chunk_bb);
                self.place_block(level, cave_air(), 1, y, i, chunk_bb);
                self.place_block(level, cave_air(), 2, y, i, chunk_bb);
                self.place_block(level, cave_air(), 3, y, i, chunk_bb);
                self.place_block(level, stone_bricks, 4, y, i, chunk_bb);
            }
            for x in 0..=4 {
                self.place_block(level, stone_bricks, x, 4, i, chunk_bb);
            }
        }
    }
}

/// `StrongholdPieces.FiveCrossing`.
pub struct FiveCrossing {
    sh: StrongholdPieceData,
    left_low: bool,
    left_high: bool,
    right_low: bool,
    right_high: bool,
}

stronghold_piece!(FiveCrossing);

impl FiveCrossing {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> FiveCrossing {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_FIVE_CROSSING,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        let left_low = random.next_boolean();
        let left_high = random.next_boolean();
        let right_low = random.next_boolean();
        let right_high = random.next_int_bounded(3) > 0;
        FiveCrossing {
            sh,
            left_low,
            left_high,
            right_low,
            right_high,
        }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<FiveCrossing, String> {
        Ok(FiveCrossing {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            left_low: tag.get_boolean_or("leftLow", false),
            left_high: tag.get_boolean_or("leftHigh", false),
            right_low: tag.get_boolean_or("rightLow", false),
            right_high: tag.get_boolean_or("rightHigh", false),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<FiveCrossing> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -4, -3, 0, 10, 9, 11, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| FiveCrossing::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for FiveCrossing {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_boolean("leftLow", self.left_low);
        tag.put_boolean("leftHigh", self.left_high);
        tag.put_boolean("rightLow", self.right_low);
        tag.put_boolean("rightHigh", self.right_high);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        let state = gen_state(start_piece);
        let mut z_off_a = 3;
        let mut z_off_b = 5;
        let orientation = self.get_orientation();
        if orientation == Some(Direction::West) || orientation == Some(Direction::North) {
            z_off_a = 8 - z_off_a;
            z_off_b = 8 - z_off_b;
        }
        self.generate_small_door_child_forward(state, builder, random, 5, 1);
        if self.left_low {
            self.generate_small_door_child_left(state, builder, random, z_off_a, 1);
        }
        if self.left_high {
            self.generate_small_door_child_left(state, builder, random, z_off_b, 7);
        }
        if self.right_low {
            self.generate_small_door_child_right(state, builder, random, z_off_a, 1);
        }
        if self.right_high {
            self.generate_small_door_child_right(state, builder, random, z_off_b, 7);
        }
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
        let air = cave_air();
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            9,
            8,
            10,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 4, 3, 0);
        if self.left_low {
            self.generate_box(level, chunk_bb, 0, 3, 1, 0, 5, 3, air, air, false);
        }
        if self.right_low {
            self.generate_box(level, chunk_bb, 9, 3, 1, 9, 5, 3, air, air, false);
        }
        if self.left_high {
            self.generate_box(level, chunk_bb, 0, 5, 7, 0, 7, 9, air, air, false);
        }
        if self.right_high {
            self.generate_box(level, chunk_bb, 9, 5, 7, 9, 7, 9, air, air, false);
        }
        self.generate_box(level, chunk_bb, 5, 1, 10, 7, 3, 10, air, air, false);
        let mut selector = SmoothStoneSelector::default();
        self.generate_box_selector(
            level,
            chunk_bb,
            1,
            2,
            1,
            8,
            2,
            6,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            1,
            5,
            4,
            4,
            9,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            8,
            1,
            5,
            8,
            4,
            9,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            1,
            4,
            7,
            3,
            4,
            9,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            1,
            3,
            5,
            3,
            3,
            6,
            false,
            random,
            &mut selector,
        );
        let slab = Block::SMOOTH_STONE_SLAB.default_state();
        self.generate_box(level, chunk_bb, 1, 3, 4, 3, 3, 4, slab, slab, false);
        self.generate_box(level, chunk_bb, 1, 4, 6, 3, 4, 6, slab, slab, false);
        self.generate_box_selector(
            level,
            chunk_bb,
            5,
            1,
            7,
            7,
            1,
            8,
            false,
            random,
            &mut selector,
        );
        self.generate_box(level, chunk_bb, 5, 1, 9, 7, 1, 9, slab, slab, false);
        self.generate_box(level, chunk_bb, 5, 2, 7, 7, 2, 7, slab, slab, false);
        self.generate_box(level, chunk_bb, 4, 5, 7, 4, 5, 9, slab, slab, false);
        self.generate_box(level, chunk_bb, 8, 5, 7, 8, 5, 9, slab, slab, false);
        let double_slab = slab.set(props::SLAB_TYPE, SlabType::Double);
        self.generate_box(
            level,
            chunk_bb,
            5,
            5,
            7,
            7,
            5,
            9,
            double_slab,
            double_slab,
            false,
        );
        self.place_block(
            level,
            Block::WALL_TORCH
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::South),
            6,
            5,
            6,
            chunk_bb,
        );
    }
}

/// `StrongholdPieces.LeftTurn`.
pub struct LeftTurn {
    sh: StrongholdPieceData,
}

stronghold_piece!(LeftTurn);

impl LeftTurn {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> LeftTurn {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_LEFT_TURN,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        LeftTurn { sh }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<LeftTurn, String> {
        Ok(LeftTurn {
            sh: StrongholdPieceData::load(piece_type, tag)?,
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<LeftTurn> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -1, 0, 5, 5, 5, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| LeftTurn::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for LeftTurn {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        let state = gen_state(start_piece);
        let orientation = self.get_orientation();
        if orientation != Some(Direction::North) && orientation != Some(Direction::East) {
            self.generate_small_door_child_right(state, builder, random, 1, 1);
        } else {
            self.generate_small_door_child_left(state, builder, random, 1, 1);
        }
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
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            4,
            4,
            4,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 1, 0);
        let orientation = self.get_orientation();
        if orientation != Some(Direction::North) && orientation != Some(Direction::East) {
            self.generate_box(
                level,
                chunk_bb,
                4,
                1,
                1,
                4,
                3,
                3,
                cave_air(),
                cave_air(),
                false,
            );
        } else {
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                1,
                0,
                3,
                3,
                cave_air(),
                cave_air(),
                false,
            );
        }
    }
}

/// `StrongholdPieces.Library`.
pub struct Library {
    sh: StrongholdPieceData,
    is_tall: bool,
}

stronghold_piece!(Library);

impl Library {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> Library {
        let mut sh =
            StrongholdPieceData::oriented(&STRONGHOLD_LIBRARY, gen_depth, bounding_box, direction);
        sh.entry_door = random_small_door(random);
        Library {
            sh,
            is_tall: bounding_box.get_y_span() > 6,
        }
    }

    fn load(piece_type: &'static StructurePieceType, tag: &CompoundTag) -> Result<Library, String> {
        Ok(Library {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            is_tall: tag.get_boolean_or("Tall", false),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<Library> {
        let mut bb =
            BoundingBox::orient_box(foot_x, foot_y, foot_z, -4, -1, 0, 14, 11, 15, direction);
        if !is_ok_box(&bb) || builder.find_collision_piece(&bb).is_some() {
            bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -4, -1, 0, 14, 6, 15, direction);
            if !is_ok_box(&bb) || builder.find_collision_piece(&bb).is_some() {
                return None;
            }
        }
        Some(Library::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for Library {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_boolean("Tall", self.is_tall);
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
        let current_height = if self.is_tall { 11 } else { 6 };
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            13,
            current_height - 1,
            14,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 4, 1, 0);
        let cobweb = Block::COBWEB.default_state();
        self.generate_maybe_box(
            level, chunk_bb, random, 0.07, 2, 1, 1, 11, 4, 13, cobweb, cobweb, false, false,
        );
        let planks = Block::OAK_PLANKS.default_state();
        let bookshelf = Block::BOOKSHELF.default_state();
        for d in 1..=13 {
            if (d - 1) % 4 == 0 {
                self.generate_box(level, chunk_bb, 1, 1, d, 1, 4, d, planks, planks, false);
                self.generate_box(level, chunk_bb, 12, 1, d, 12, 4, d, planks, planks, false);
                self.place_block(
                    level,
                    Block::WALL_TORCH
                        .default_state()
                        .set(props::HORIZONTAL_FACING, Direction::East),
                    2,
                    3,
                    d,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    Block::WALL_TORCH
                        .default_state()
                        .set(props::HORIZONTAL_FACING, Direction::West),
                    11,
                    3,
                    d,
                    chunk_bb,
                );
                if self.is_tall {
                    self.generate_box(level, chunk_bb, 1, 6, d, 1, 9, d, planks, planks, false);
                    self.generate_box(level, chunk_bb, 12, 6, d, 12, 9, d, planks, planks, false);
                }
            } else {
                self.generate_box(
                    level, chunk_bb, 1, 1, d, 1, 4, d, bookshelf, bookshelf, false,
                );
                self.generate_box(
                    level, chunk_bb, 12, 1, d, 12, 4, d, bookshelf, bookshelf, false,
                );
                if self.is_tall {
                    self.generate_box(
                        level, chunk_bb, 1, 6, d, 1, 9, d, bookshelf, bookshelf, false,
                    );
                    self.generate_box(
                        level, chunk_bb, 12, 6, d, 12, 9, d, bookshelf, bookshelf, false,
                    );
                }
            }
        }
        let mut dx = 3;
        while dx < 12 {
            self.generate_box(
                level, chunk_bb, 3, 1, dx, 4, 3, dx, bookshelf, bookshelf, false,
            );
            self.generate_box(
                level, chunk_bb, 6, 1, dx, 7, 3, dx, bookshelf, bookshelf, false,
            );
            self.generate_box(
                level, chunk_bb, 9, 1, dx, 10, 3, dx, bookshelf, bookshelf, false,
            );
            dx += 2;
        }
        if self.is_tall {
            self.generate_box(level, chunk_bb, 1, 5, 1, 3, 5, 13, planks, planks, false);
            self.generate_box(level, chunk_bb, 10, 5, 1, 12, 5, 13, planks, planks, false);
            self.generate_box(level, chunk_bb, 4, 5, 1, 9, 5, 2, planks, planks, false);
            self.generate_box(level, chunk_bb, 4, 5, 12, 9, 5, 13, planks, planks, false);
            self.place_block(level, planks, 9, 5, 11, chunk_bb);
            self.place_block(level, planks, 8, 5, 11, chunk_bb);
            self.place_block(level, planks, 9, 5, 10, chunk_bb);
            let fence = Block::OAK_FENCE.default_state();
            let we_fence = fence.set(props::WEST, true).set(props::EAST, true);
            let ns_fence = fence.set(props::NORTH, true).set(props::SOUTH, true);
            self.generate_box(
                level, chunk_bb, 3, 6, 3, 3, 6, 11, ns_fence, ns_fence, false,
            );
            self.generate_box(
                level, chunk_bb, 10, 6, 3, 10, 6, 9, ns_fence, ns_fence, false,
            );
            self.generate_box(level, chunk_bb, 4, 6, 2, 9, 6, 2, we_fence, we_fence, false);
            self.generate_box(
                level, chunk_bb, 4, 6, 12, 7, 6, 12, we_fence, we_fence, false,
            );
            self.place_block(
                level,
                fence.set(props::NORTH, true).set(props::EAST, true),
                3,
                6,
                2,
                chunk_bb,
            );
            self.place_block(
                level,
                fence.set(props::SOUTH, true).set(props::EAST, true),
                3,
                6,
                12,
                chunk_bb,
            );
            self.place_block(
                level,
                fence.set(props::NORTH, true).set(props::WEST, true),
                10,
                6,
                2,
                chunk_bb,
            );
            for i in 0..=2 {
                self.place_block(
                    level,
                    fence.set(props::SOUTH, true).set(props::WEST, true),
                    8 + i,
                    6,
                    12 - i,
                    chunk_bb,
                );
                if i != 2 {
                    self.place_block(
                        level,
                        fence.set(props::NORTH, true).set(props::EAST, true),
                        8 + i,
                        6,
                        11 - i,
                        chunk_bb,
                    );
                }
            }
            let ladder = Block::LADDER
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::South);
            for y in 1..=7 {
                self.place_block(level, ladder, 10, y, 13, chunk_bb);
            }
            let e_fence = fence.set(props::EAST, true);
            self.place_block(level, e_fence, 6, 9, 7, chunk_bb);
            let w_fence = fence.set(props::WEST, true);
            self.place_block(level, w_fence, 7, 9, 7, chunk_bb);
            self.place_block(level, e_fence, 6, 8, 7, chunk_bb);
            self.place_block(level, w_fence, 7, 8, 7, chunk_bb);
            let nswe_fence = ns_fence.set(props::WEST, true).set(props::EAST, true);
            self.place_block(level, nswe_fence, 6, 7, 7, chunk_bb);
            self.place_block(level, nswe_fence, 7, 7, 7, chunk_bb);
            self.place_block(level, e_fence, 5, 7, 7, chunk_bb);
            self.place_block(level, w_fence, 8, 7, 7, chunk_bb);
            self.place_block(level, e_fence.set(props::NORTH, true), 6, 7, 6, chunk_bb);
            self.place_block(level, e_fence.set(props::SOUTH, true), 6, 7, 8, chunk_bb);
            self.place_block(level, w_fence.set(props::NORTH, true), 7, 7, 6, chunk_bb);
            self.place_block(level, w_fence.set(props::SOUTH, true), 7, 7, 8, chunk_bb);
            let torch = Block::TORCH.default_state();
            self.place_block(level, torch, 5, 8, 7, chunk_bb);
            self.place_block(level, torch, 8, 8, 7, chunk_bb);
            self.place_block(level, torch, 6, 8, 6, chunk_bb);
            self.place_block(level, torch, 6, 8, 8, chunk_bb);
            self.place_block(level, torch, 7, 8, 6, chunk_bb);
            self.place_block(level, torch, 7, 8, 8, chunk_bb);
        }
        self.create_chest(level, chunk_bb, random, 3, 3, 5, LIBRARY_LOOT);
        if self.is_tall {
            self.place_block(level, cave_air(), 12, 9, 1, chunk_bb);
            self.create_chest(level, chunk_bb, random, 12, 8, 1, LIBRARY_LOOT);
        }
    }
}

/// `StrongholdPieces.PortalRoom`.
pub struct PortalRoom {
    sh: StrongholdPieceData,
    has_placed_spawner: bool,
}

stronghold_piece!(PortalRoom);

impl PortalRoom {
    fn new(gen_depth: i32, bounding_box: BoundingBox, direction: Direction) -> PortalRoom {
        PortalRoom {
            sh: StrongholdPieceData::oriented(
                &STRONGHOLD_PORTAL_ROOM,
                gen_depth,
                bounding_box,
                direction,
            ),
            has_placed_spawner: false,
        }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<PortalRoom, String> {
        Ok(PortalRoom {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            has_placed_spawner: tag.get_boolean_or("Mob", false),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<PortalRoom> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -4, -1, 0, 11, 8, 16, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| PortalRoom::new(gen_depth, bb, direction))
    }
}

impl StructurePiece for PortalRoom {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_boolean("Mob", self.has_placed_spawner);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        _builder: &mut StructurePiecesBuilder,
        _random: &mut dyn RandomSource,
    ) {
        if start_piece.is_some() {
            gen_state(start_piece).portal_room_locator = Some(self.get_locator_position());
        }
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
        let mut selector = SmoothStoneSelector::default();
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            10,
            7,
            15,
            false,
            random,
            &mut selector,
        );
        self.generate_small_door(level, chunk_bb, SmallDoorType::Grates, 4, 1, 0);
        self.generate_box_selector(
            level,
            chunk_bb,
            1,
            6,
            1,
            1,
            6,
            14,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            9,
            6,
            1,
            9,
            6,
            14,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            2,
            6,
            1,
            8,
            6,
            2,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            2,
            6,
            14,
            8,
            6,
            14,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            1,
            1,
            1,
            2,
            1,
            4,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            8,
            1,
            1,
            9,
            1,
            4,
            false,
            random,
            &mut selector,
        );
        let lava = Block::LAVA.default_state();
        self.generate_box(level, chunk_bb, 1, 1, 1, 1, 1, 3, lava, lava, false);
        self.generate_box(level, chunk_bb, 9, 1, 1, 9, 1, 3, lava, lava, false);
        self.generate_box_selector(
            level,
            chunk_bb,
            3,
            1,
            8,
            7,
            1,
            12,
            false,
            random,
            &mut selector,
        );
        self.generate_box(level, chunk_bb, 4, 1, 9, 6, 1, 11, lava, lava, false);
        let ns_bars = Block::IRON_BARS
            .default_state()
            .set(props::NORTH, true)
            .set(props::SOUTH, true);
        let we_bars = Block::IRON_BARS
            .default_state()
            .set(props::WEST, true)
            .set(props::EAST, true);
        let mut z = 3;
        while z < 14 {
            self.generate_box(level, chunk_bb, 0, 3, z, 0, 4, z, ns_bars, ns_bars, false);
            self.generate_box(level, chunk_bb, 10, 3, z, 10, 4, z, ns_bars, ns_bars, false);
            z += 2;
        }
        let mut x = 2;
        while x < 9 {
            self.generate_box(level, chunk_bb, x, 3, 15, x, 4, 15, we_bars, we_bars, false);
            x += 2;
        }
        let stairs = Block::STONE_BRICK_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::North);
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            1,
            5,
            6,
            1,
            7,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            2,
            6,
            6,
            2,
            7,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            3,
            7,
            6,
            3,
            7,
            false,
            random,
            &mut selector,
        );
        for x in 4..=6 {
            self.place_block(level, stairs, x, 1, 4, chunk_bb);
            self.place_block(level, stairs, x, 2, 5, chunk_bb);
            self.place_block(level, stairs, x, 3, 6, chunk_bb);
        }
        let frame = Block::END_PORTAL_FRAME.default_state();
        let north_frame = frame.set(props::HORIZONTAL_FACING, Direction::North);
        let south_frame = frame.set(props::HORIZONTAL_FACING, Direction::South);
        let east_frame = frame.set(props::HORIZONTAL_FACING, Direction::East);
        let west_frame = frame.set(props::HORIZONTAL_FACING, Direction::West);
        let mut all_eyes = true;
        let mut eyes = [false; 12];
        for eye in &mut eyes {
            *eye = random.next_float() > 0.9;
            all_eyes &= *eye;
        }
        self.place_block(
            level,
            north_frame.set(props::EYE, eyes[0]),
            4,
            3,
            8,
            chunk_bb,
        );
        self.place_block(
            level,
            north_frame.set(props::EYE, eyes[1]),
            5,
            3,
            8,
            chunk_bb,
        );
        self.place_block(
            level,
            north_frame.set(props::EYE, eyes[2]),
            6,
            3,
            8,
            chunk_bb,
        );
        self.place_block(
            level,
            south_frame.set(props::EYE, eyes[3]),
            4,
            3,
            12,
            chunk_bb,
        );
        self.place_block(
            level,
            south_frame.set(props::EYE, eyes[4]),
            5,
            3,
            12,
            chunk_bb,
        );
        self.place_block(
            level,
            south_frame.set(props::EYE, eyes[5]),
            6,
            3,
            12,
            chunk_bb,
        );
        self.place_block(
            level,
            east_frame.set(props::EYE, eyes[6]),
            3,
            3,
            9,
            chunk_bb,
        );
        self.place_block(
            level,
            east_frame.set(props::EYE, eyes[7]),
            3,
            3,
            10,
            chunk_bb,
        );
        self.place_block(
            level,
            east_frame.set(props::EYE, eyes[8]),
            3,
            3,
            11,
            chunk_bb,
        );
        self.place_block(
            level,
            west_frame.set(props::EYE, eyes[9]),
            7,
            3,
            9,
            chunk_bb,
        );
        self.place_block(
            level,
            west_frame.set(props::EYE, eyes[10]),
            7,
            3,
            10,
            chunk_bb,
        );
        self.place_block(
            level,
            west_frame.set(props::EYE, eyes[11]),
            7,
            3,
            11,
            chunk_bb,
        );
        if all_eyes {
            let portal = Block::END_PORTAL.default_state();
            for portal_z in 9..=11 {
                for portal_x in 4..=6 {
                    self.place_block(level, portal, portal_x, 3, portal_z, chunk_bb);
                }
            }
        }
        if !self.has_placed_spawner {
            let pos = self.get_world_pos(5, 3, 6);
            if chunk_bb.is_inside(&pos) {
                self.has_placed_spawner = true;
                level.set_block_flags(pos, Block::SPAWNER.default_state(), 2);
                if level.get_block_state(pos).is_block(Block::SPAWNER) {
                    level.set_block_entity_nbt(spawner_tag(pos, "minecraft:silverfish"));
                }
            }
        }
    }
}

/// `StrongholdPieces.PrisonHall`.
pub struct PrisonHall {
    sh: StrongholdPieceData,
}

stronghold_piece!(PrisonHall);

impl PrisonHall {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> PrisonHall {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_PRISON_HALL,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        PrisonHall { sh }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<PrisonHall, String> {
        Ok(PrisonHall {
            sh: StrongholdPieceData::load(piece_type, tag)?,
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<PrisonHall> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -1, 0, 9, 5, 11, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| PrisonHall::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for PrisonHall {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        self.generate_small_door_child_forward(gen_state(start_piece), builder, random, 1, 1);
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
        let mut selector = SmoothStoneSelector::default();
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            8,
            4,
            10,
            true,
            random,
            &mut selector,
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 1, 0);
        self.generate_box(
            level,
            chunk_bb,
            1,
            1,
            10,
            3,
            3,
            10,
            cave_air(),
            cave_air(),
            false,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            1,
            1,
            4,
            3,
            1,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            1,
            3,
            4,
            3,
            3,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            1,
            7,
            4,
            3,
            7,
            false,
            random,
            &mut selector,
        );
        self.generate_box_selector(
            level,
            chunk_bb,
            4,
            1,
            9,
            4,
            3,
            9,
            false,
            random,
            &mut selector,
        );
        let bars = Block::IRON_BARS.default_state();
        let ns_bars = bars.set(props::NORTH, true).set(props::SOUTH, true);
        let we_bars = bars.set(props::WEST, true).set(props::EAST, true);
        for y in 1..=3 {
            self.place_block(level, ns_bars, 4, y, 4, chunk_bb);
            self.place_block(level, ns_bars.set(props::EAST, true), 4, y, 5, chunk_bb);
            self.place_block(level, ns_bars, 4, y, 6, chunk_bb);
            self.place_block(level, we_bars, 5, y, 5, chunk_bb);
            self.place_block(level, we_bars, 6, y, 5, chunk_bb);
            self.place_block(level, we_bars, 7, y, 5, chunk_bb);
        }
        self.place_block(level, ns_bars, 4, 3, 2, chunk_bb);
        self.place_block(level, ns_bars, 4, 3, 8, chunk_bb);
        let door_bottom = Block::IRON_DOOR
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::West);
        let door_top = door_bottom.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper);
        self.place_block(level, door_bottom, 4, 1, 2, chunk_bb);
        self.place_block(level, door_top, 4, 2, 2, chunk_bb);
        self.place_block(level, door_bottom, 4, 1, 8, chunk_bb);
        self.place_block(level, door_top, 4, 2, 8, chunk_bb);
    }
}

/// `StrongholdPieces.RightTurn`.
pub struct RightTurn {
    sh: StrongholdPieceData,
}

stronghold_piece!(RightTurn);

impl RightTurn {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> RightTurn {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_RIGHT_TURN,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        RightTurn { sh }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<RightTurn, String> {
        Ok(RightTurn {
            sh: StrongholdPieceData::load(piece_type, tag)?,
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<RightTurn> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -1, 0, 5, 5, 5, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| RightTurn::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for RightTurn {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        let state = gen_state(start_piece);
        let orientation = self.get_orientation();
        if orientation != Some(Direction::North) && orientation != Some(Direction::East) {
            self.generate_small_door_child_left(state, builder, random, 1, 1);
        } else {
            self.generate_small_door_child_right(state, builder, random, 1, 1);
        }
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
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            4,
            4,
            4,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 1, 0);
        let orientation = self.get_orientation();
        if orientation != Some(Direction::North) && orientation != Some(Direction::East) {
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                1,
                0,
                3,
                3,
                cave_air(),
                cave_air(),
                false,
            );
        } else {
            self.generate_box(
                level,
                chunk_bb,
                4,
                1,
                1,
                4,
                3,
                3,
                cave_air(),
                cave_air(),
                false,
            );
        }
    }
}

/// `StrongholdPieces.RoomCrossing`.
pub struct RoomCrossing {
    sh: StrongholdPieceData,
    type_: i32,
}

stronghold_piece!(RoomCrossing);

impl RoomCrossing {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> RoomCrossing {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_ROOM_CROSSING,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        RoomCrossing {
            sh,
            type_: random.next_int_bounded(5),
        }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<RoomCrossing, String> {
        Ok(RoomCrossing {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            type_: tag.get_int_or("Type", 0),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<RoomCrossing> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -4, -1, 0, 11, 7, 11, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| RoomCrossing::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for RoomCrossing {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_int("Type", self.type_);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        let state = gen_state(start_piece);
        self.generate_small_door_child_forward(state, builder, random, 4, 1);
        self.generate_small_door_child_left(state, builder, random, 1, 4);
        self.generate_small_door_child_right(state, builder, random, 1, 4);
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
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            10,
            6,
            10,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 4, 1, 0);
        self.generate_box(
            level,
            chunk_bb,
            4,
            1,
            10,
            6,
            3,
            10,
            cave_air(),
            cave_air(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            0,
            1,
            4,
            0,
            3,
            6,
            cave_air(),
            cave_air(),
            false,
        );
        self.generate_box(
            level,
            chunk_bb,
            10,
            1,
            4,
            10,
            3,
            6,
            cave_air(),
            cave_air(),
            false,
        );
        let stone_bricks = Block::STONE_BRICKS.default_state();
        let cobblestone = Block::COBBLESTONE.default_state();
        let planks = Block::OAK_PLANKS.default_state();
        let wall_torch = Block::WALL_TORCH.default_state();
        match self.type_ {
            0 => {
                self.place_block(level, stone_bricks, 5, 1, 5, chunk_bb);
                self.place_block(level, stone_bricks, 5, 2, 5, chunk_bb);
                self.place_block(level, stone_bricks, 5, 3, 5, chunk_bb);
                self.place_block(
                    level,
                    wall_torch.set(props::HORIZONTAL_FACING, Direction::West),
                    4,
                    3,
                    5,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    wall_torch.set(props::HORIZONTAL_FACING, Direction::East),
                    6,
                    3,
                    5,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    wall_torch.set(props::HORIZONTAL_FACING, Direction::South),
                    5,
                    3,
                    4,
                    chunk_bb,
                );
                self.place_block(
                    level,
                    wall_torch.set(props::HORIZONTAL_FACING, Direction::North),
                    5,
                    3,
                    6,
                    chunk_bb,
                );
                let slab = Block::SMOOTH_STONE_SLAB.default_state();
                self.place_block(level, slab, 4, 1, 4, chunk_bb);
                self.place_block(level, slab, 4, 1, 5, chunk_bb);
                self.place_block(level, slab, 4, 1, 6, chunk_bb);
                self.place_block(level, slab, 6, 1, 4, chunk_bb);
                self.place_block(level, slab, 6, 1, 5, chunk_bb);
                self.place_block(level, slab, 6, 1, 6, chunk_bb);
                self.place_block(level, slab, 5, 1, 4, chunk_bb);
                self.place_block(level, slab, 5, 1, 6, chunk_bb);
            }
            1 => {
                for i in 0..5 {
                    self.place_block(level, stone_bricks, 3, 1, 3 + i, chunk_bb);
                    self.place_block(level, stone_bricks, 7, 1, 3 + i, chunk_bb);
                    self.place_block(level, stone_bricks, 3 + i, 1, 3, chunk_bb);
                    self.place_block(level, stone_bricks, 3 + i, 1, 7, chunk_bb);
                }
                self.place_block(level, stone_bricks, 5, 1, 5, chunk_bb);
                self.place_block(level, stone_bricks, 5, 2, 5, chunk_bb);
                self.place_block(level, stone_bricks, 5, 3, 5, chunk_bb);
                self.place_block(level, Block::WATER.default_state(), 5, 4, 5, chunk_bb);
            }
            2 => {
                for z in 1..=9 {
                    self.place_block(level, cobblestone, 1, 3, z, chunk_bb);
                    self.place_block(level, cobblestone, 9, 3, z, chunk_bb);
                }
                for x in 1..=9 {
                    self.place_block(level, cobblestone, x, 3, 1, chunk_bb);
                    self.place_block(level, cobblestone, x, 3, 9, chunk_bb);
                }
                self.place_block(level, cobblestone, 5, 1, 4, chunk_bb);
                self.place_block(level, cobblestone, 5, 1, 6, chunk_bb);
                self.place_block(level, cobblestone, 5, 3, 4, chunk_bb);
                self.place_block(level, cobblestone, 5, 3, 6, chunk_bb);
                self.place_block(level, cobblestone, 4, 1, 5, chunk_bb);
                self.place_block(level, cobblestone, 6, 1, 5, chunk_bb);
                self.place_block(level, cobblestone, 4, 3, 5, chunk_bb);
                self.place_block(level, cobblestone, 6, 3, 5, chunk_bb);
                for y in 1..=3 {
                    self.place_block(level, cobblestone, 4, y, 4, chunk_bb);
                    self.place_block(level, cobblestone, 6, y, 4, chunk_bb);
                    self.place_block(level, cobblestone, 4, y, 6, chunk_bb);
                    self.place_block(level, cobblestone, 6, y, 6, chunk_bb);
                }
                self.place_block(level, wall_torch, 5, 3, 5, chunk_bb);
                for z in 2..=8 {
                    self.place_block(level, planks, 2, 3, z, chunk_bb);
                    self.place_block(level, planks, 3, 3, z, chunk_bb);
                    if z <= 3 || z >= 7 {
                        self.place_block(level, planks, 4, 3, z, chunk_bb);
                        self.place_block(level, planks, 5, 3, z, chunk_bb);
                        self.place_block(level, planks, 6, 3, z, chunk_bb);
                    }
                    self.place_block(level, planks, 7, 3, z, chunk_bb);
                    self.place_block(level, planks, 8, 3, z, chunk_bb);
                }
                let ladder = Block::LADDER
                    .default_state()
                    .set(props::HORIZONTAL_FACING, Direction::West);
                self.place_block(level, ladder, 9, 1, 3, chunk_bb);
                self.place_block(level, ladder, 9, 2, 3, chunk_bb);
                self.place_block(level, ladder, 9, 3, 3, chunk_bb);
                self.create_chest(level, chunk_bb, random, 3, 4, 8, CROSSING_LOOT);
            }
            _ => {}
        }
    }
}

/// `StrongholdPieces.StairsDown`.
pub struct StairsDown {
    sh: StrongholdPieceData,
    is_source: bool,
}

stronghold_piece!(StairsDown);

impl StairsDown {
    /// `StairsDown(type, genDepth, west, north, direction)`: the source (start) staircase.
    fn new_source(
        piece_type: &'static StructurePieceType,
        gen_depth: i32,
        west: i32,
        north: i32,
        direction: Direction,
    ) -> StairsDown {
        let sh = StrongholdPieceData::oriented(
            piece_type,
            gen_depth,
            make_bounding_box(west, MAGIC_START_Y, north, direction, 5, 11, 5),
            direction,
        );
        StairsDown {
            sh,
            is_source: true,
        }
    }

    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> StairsDown {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_STAIRS_DOWN,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        StairsDown {
            sh,
            is_source: false,
        }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<StairsDown, String> {
        Ok(StairsDown {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            is_source: tag.get_boolean_or("Source", false),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<StairsDown> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -7, 0, 5, 11, 5, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| StairsDown::new(gen_depth, random, bb, direction))
    }

    fn save(&self, tag: &mut CompoundTag) {
        self.sh.save(tag);
        tag.put_boolean("Source", self.is_source);
    }

    /// `addChildren` with the start piece's state at hand.
    fn add_children_with(
        &self,
        state: &mut StrongholdGenState,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        if self.is_source {
            state.imposed_piece = Some(PieceClass::FiveCrossing);
        }
        self.generate_small_door_child_forward(state, builder, random, 1, 1);
    }

    fn post_process_stairs(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
    ) {
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            4,
            10,
            4,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 7, 0);
        self.generate_small_door(level, chunk_bb, SmallDoorType::Opening, 1, 1, 4);
        let stone_bricks = Block::STONE_BRICKS.default_state();
        let slab = Block::SMOOTH_STONE_SLAB.default_state();
        self.place_block(level, stone_bricks, 2, 6, 1, chunk_bb);
        self.place_block(level, stone_bricks, 1, 5, 1, chunk_bb);
        self.place_block(level, slab, 1, 6, 1, chunk_bb);
        self.place_block(level, stone_bricks, 1, 5, 2, chunk_bb);
        self.place_block(level, stone_bricks, 1, 4, 3, chunk_bb);
        self.place_block(level, slab, 1, 5, 3, chunk_bb);
        self.place_block(level, stone_bricks, 2, 4, 3, chunk_bb);
        self.place_block(level, stone_bricks, 3, 3, 3, chunk_bb);
        self.place_block(level, slab, 3, 4, 3, chunk_bb);
        self.place_block(level, stone_bricks, 3, 3, 2, chunk_bb);
        self.place_block(level, stone_bricks, 3, 2, 1, chunk_bb);
        self.place_block(level, slab, 3, 3, 1, chunk_bb);
        self.place_block(level, stone_bricks, 2, 2, 1, chunk_bb);
        self.place_block(level, stone_bricks, 1, 1, 1, chunk_bb);
        self.place_block(level, slab, 1, 2, 1, chunk_bb);
        self.place_block(level, stone_bricks, 1, 1, 2, chunk_bb);
        self.place_block(level, slab, 1, 1, 3, chunk_bb);
    }
}

impl StructurePiece for StairsDown {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.save(tag);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        self.add_children_with(gen_state(start_piece), builder, random);
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
        self.post_process_stairs(level, random, chunk_bb);
    }
}

/// `StrongholdPieces.StartPiece` (a `StairsDown` that owns the generation state).
pub struct StartPiece {
    stairs: StairsDown,
    pub state: StrongholdGenState,
}

impl StartPiece {
    /// `StartPiece(random, west, north)` (after `resetPieces()`).
    pub fn new(random: &mut dyn RandomSource, west: i32, north: i32) -> StartPiece {
        let direction = get_random_horizontal_direction(random);
        let stairs = StairsDown::new_source(&STRONGHOLD_START, 0, west, north, direction);
        let state = StrongholdGenState::new(
            stairs.sh.base.bounding_box.min_x(),
            stairs.sh.base.bounding_box.min_z(),
        );
        StartPiece { stairs, state }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<StartPiece, String> {
        let stairs = StairsDown::load(piece_type, tag)?;
        let state = StrongholdGenState::new(
            stairs.sh.base.bounding_box.min_x(),
            stairs.sh.base.bounding_box.min_z(),
        );
        Ok(StartPiece { stairs, state })
    }
}

impl StructurePiece for StartPiece {
    fn base(&self) -> &PieceBase {
        &self.stairs.sh.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.stairs.sh.base
    }

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.stairs.save(tag);
    }

    /// The start piece expands itself (vanilla passes it as its own `startPiece`).
    fn add_children(
        &mut self,
        _start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        self.stairs
            .add_children_with(&mut self.state, builder, random);
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
        self.stairs.post_process_stairs(level, random, chunk_bb);
    }

    /// `portalRoomPiece != null ? portalRoomPiece.getLocatorPosition() : super.getLocatorPosition()`.
    fn get_locator_position(&self) -> BlockPos {
        self.state
            .portal_room_locator
            .unwrap_or_else(|| self.stairs.base().bounding_box.get_center())
    }

    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.stairs.base_mut().bounding_box.move_(dx, dy, dz);
        if let Some(locator) = &mut self.state.portal_room_locator {
            *locator = locator.offset(dx, dy, dz);
        }
    }
}

/// `StrongholdPieces.Straight`.
pub struct Straight {
    sh: StrongholdPieceData,
    left_child: bool,
    right_child: bool,
}

stronghold_piece!(Straight);

impl Straight {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> Straight {
        let mut sh =
            StrongholdPieceData::oriented(&STRONGHOLD_STRAIGHT, gen_depth, bounding_box, direction);
        sh.entry_door = random_small_door(random);
        let left_child = random.next_int_bounded(2) == 0;
        let right_child = random.next_int_bounded(2) == 0;
        Straight {
            sh,
            left_child,
            right_child,
        }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<Straight, String> {
        Ok(Straight {
            sh: StrongholdPieceData::load(piece_type, tag)?,
            left_child: tag.get_boolean_or("Left", false),
            right_child: tag.get_boolean_or("Right", false),
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<Straight> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -1, 0, 5, 5, 7, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| Straight::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for Straight {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
        tag.put_boolean("Left", self.left_child);
        tag.put_boolean("Right", self.right_child);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        let state = gen_state(start_piece);
        self.generate_small_door_child_forward(state, builder, random, 1, 1);
        if self.left_child {
            self.generate_small_door_child_left(state, builder, random, 1, 2);
        }
        if self.right_child {
            self.generate_small_door_child_right(state, builder, random, 1, 2);
        }
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
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            4,
            4,
            6,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 1, 0);
        self.generate_small_door(level, chunk_bb, SmallDoorType::Opening, 1, 1, 6);
        let east_torch = Block::WALL_TORCH
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::East);
        let west_torch = Block::WALL_TORCH
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::West);
        self.maybe_generate_block(level, chunk_bb, random, 0.1, 1, 2, 1, east_torch);
        self.maybe_generate_block(level, chunk_bb, random, 0.1, 3, 2, 1, west_torch);
        self.maybe_generate_block(level, chunk_bb, random, 0.1, 1, 2, 5, east_torch);
        self.maybe_generate_block(level, chunk_bb, random, 0.1, 3, 2, 5, west_torch);
        if self.left_child {
            self.generate_box(
                level,
                chunk_bb,
                0,
                1,
                2,
                0,
                3,
                4,
                cave_air(),
                cave_air(),
                false,
            );
        }
        if self.right_child {
            self.generate_box(
                level,
                chunk_bb,
                4,
                1,
                2,
                4,
                3,
                4,
                cave_air(),
                cave_air(),
                false,
            );
        }
    }
}

/// `StrongholdPieces.StraightStairsDown`.
pub struct StraightStairsDown {
    sh: StrongholdPieceData,
}

stronghold_piece!(StraightStairsDown);

impl StraightStairsDown {
    fn new(
        gen_depth: i32,
        random: &mut dyn RandomSource,
        bounding_box: BoundingBox,
        direction: Direction,
    ) -> StraightStairsDown {
        let mut sh = StrongholdPieceData::oriented(
            &STRONGHOLD_STRAIGHT_STAIRS_DOWN,
            gen_depth,
            bounding_box,
            direction,
        );
        sh.entry_door = random_small_door(random);
        StraightStairsDown { sh }
    }

    fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<StraightStairsDown, String> {
        Ok(StraightStairsDown {
            sh: StrongholdPieceData::load(piece_type, tag)?,
        })
    }

    fn create_piece(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
        gen_depth: i32,
    ) -> Option<StraightStairsDown> {
        let bb = BoundingBox::orient_box(foot_x, foot_y, foot_z, -1, -7, 0, 5, 11, 8, direction);
        (is_ok_box(&bb) && builder.find_collision_piece(&bb).is_none())
            .then(|| StraightStairsDown::new(gen_depth, random, bb, direction))
    }
}

impl StructurePiece for StraightStairsDown {
    base_accessors!();

    fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.sh.save(tag);
    }

    fn add_children(
        &mut self,
        start_piece: Option<&mut dyn StructurePiece>,
        builder: &mut StructurePiecesBuilder,
        random: &mut dyn RandomSource,
    ) {
        self.generate_small_door_child_forward(gen_state(start_piece), builder, random, 1, 1);
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
        self.generate_box_selector(
            level,
            chunk_bb,
            0,
            0,
            0,
            4,
            10,
            7,
            true,
            random,
            &mut SmoothStoneSelector::default(),
        );
        self.generate_small_door(level, chunk_bb, self.sh.entry_door, 1, 7, 0);
        self.generate_small_door(level, chunk_bb, SmallDoorType::Opening, 1, 1, 7);
        let stairs = Block::COBBLESTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::South);
        let stone_bricks = Block::STONE_BRICKS.default_state();
        for i in 0..6 {
            self.place_block(level, stairs, 1, 6 - i, 1 + i, chunk_bb);
            self.place_block(level, stairs, 2, 6 - i, 1 + i, chunk_bb);
            self.place_block(level, stairs, 3, 6 - i, 1 + i, chunk_bb);
            if i < 5 {
                self.place_block(level, stone_bricks, 1, 5 - i, 1 + i, chunk_bb);
                self.place_block(level, stone_bricks, 2, 5 - i, 1 + i, chunk_bb);
                self.place_block(level, stone_bricks, 3, 5 - i, 1 + i, chunk_bb);
            }
        }
    }
}
