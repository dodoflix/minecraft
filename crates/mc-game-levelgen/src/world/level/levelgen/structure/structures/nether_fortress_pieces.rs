//! `NetherFortressPieces`: the pieces of the nether fortress and the weighted piece generation shared by all of them
//! (`NetherBridgePiece`).
//!
//! `StartPiece` carries the generation state (`availableBridgePieces`, `availableCastlePieces`, `previousPiece`,
//! `pendingChildren`); every other piece reaches it through the `startPiece` argument of `add_children`. The pending
//! children are the builder indices of pieces whose children are still to be generated
//! ([`StructurePiecesBuilder::take_out`] hands such a piece out mutably next to the builder).

use crate::world::level::levelgen::block_entity_data::spawner_tag;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::piece::{
    PieceBase, StructurePiece, StructurePieceSerializationContext, StructurePieceType, get_random_horizontal_direction, make_bounding_box,
    STRUCTURE_PIECE_TYPES,
};
use crate::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{RandomSource, SingleThreadedRandomSource};
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_world::ticks::TickPriority;

const MAX_DEPTH: i32 = 30;
const MAGIC_START_Y: i32 = 64;
const CHEST_LOOT_TABLE: &str = "minecraft:chests/nether_bridge";

/// The `PieceWeight.pieceClass` identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieceKind {
    BridgeStraight,
    BridgeCrossing,
    RoomCrossing,
    StairsRoom,
    MonsterThrone,
    CastleEntrance,
    CastleSmallCorridor,
    CastleSmallCorridorCrossing,
    CastleSmallCorridorRightTurn,
    CastleSmallCorridorLeftTurn,
    CastleCorridorStairs,
    CastleCorridorTBalcony,
    CastleStalkRoom,
}

/// `PieceWeight` (`placeCount` is per start piece here: vanilla resets the shared static in `StartPiece`'s constructor).
#[derive(Clone, Debug)]
struct PieceWeight {
    kind: PieceKind,
    weight: i32,
    place_count: i32,
    max_place_count: i32,
    allow_in_row: bool,
}

impl PieceWeight {
    const fn new(kind: PieceKind, weight: i32, max_place_count: i32, allow_in_row: bool) -> PieceWeight {
        PieceWeight { kind, weight, place_count: 0, max_place_count, allow_in_row }
    }

    fn do_place(&self, _depth: i32) -> bool {
        self.max_place_count == 0 || self.place_count < self.max_place_count
    }

    fn is_valid(&self) -> bool {
        self.max_place_count == 0 || self.place_count < self.max_place_count
    }
}

/// `BRIDGE_PIECE_WEIGHTS`.
const BRIDGE_PIECE_WEIGHTS: [PieceWeight; 6] = [
    PieceWeight::new(PieceKind::BridgeStraight, 30, 0, true),
    PieceWeight::new(PieceKind::BridgeCrossing, 10, 4, false),
    PieceWeight::new(PieceKind::RoomCrossing, 10, 4, false),
    PieceWeight::new(PieceKind::StairsRoom, 10, 3, false),
    PieceWeight::new(PieceKind::MonsterThrone, 5, 2, false),
    PieceWeight::new(PieceKind::CastleEntrance, 5, 1, false),
];

/// `CASTLE_PIECE_WEIGHTS`.
const CASTLE_PIECE_WEIGHTS: [PieceWeight; 7] = [
    PieceWeight::new(PieceKind::CastleSmallCorridor, 25, 0, true),
    PieceWeight::new(PieceKind::CastleSmallCorridorCrossing, 15, 5, false),
    PieceWeight::new(PieceKind::CastleSmallCorridorRightTurn, 5, 10, false),
    PieceWeight::new(PieceKind::CastleSmallCorridorLeftTurn, 5, 10, false),
    PieceWeight::new(PieceKind::CastleCorridorStairs, 10, 3, true),
    PieceWeight::new(PieceKind::CastleCorridorTBalcony, 7, 2, false),
    PieceWeight::new(PieceKind::CastleStalkRoom, 5, 2, false),
];

/// `NetherBridgePiece.isOkBox(box)`.
fn is_ok_box(bounding_box: &BoundingBox) -> bool {
    bounding_box.min_y() > 10
}

/// The common head of every `createPiece`: the oriented box, when it is ok and collides with no piece yet.
#[allow(clippy::too_many_arguments)]
fn create_box(
    builder: &StructurePiecesBuilder,
    foot: (i32, i32, i32),
    offset: (i32, i32, i32),
    size: (i32, i32, i32),
    direction: Direction,
) -> Option<BoundingBox> {
    let bounding_box = BoundingBox::orient_box(foot.0, foot.1, foot.2, offset.0, offset.1, offset.2, size.0, size.1, size.2, direction);
    (is_ok_box(&bounding_box) && builder.find_collision_piece(&bounding_box).is_none()).then_some(bounding_box)
}

/// `findAndCreateBridgePieceFactory(piece, builder, random, footX, footY, footZ, direction, depth)`.
#[allow(clippy::too_many_arguments)]
fn find_and_create_bridge_piece_factory(
    kind: PieceKind,
    builder: &StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
) -> Option<Box<dyn StructurePiece>> {
    let foot = (foot_x, foot_y, foot_z);
    let create = |offset, size| create_box(builder, foot, offset, size, direction);
    Some(match kind {
        PieceKind::BridgeStraight => Box::new(BridgeStraight::new(depth, create((-1, -3, 0), (5, 10, 19))?, direction)),
        PieceKind::BridgeCrossing => Box::new(BridgeCrossing::new(depth, create((-8, -3, 0), (19, 10, 19))?, direction)),
        PieceKind::RoomCrossing => Box::new(RoomCrossing::new(depth, create((-2, 0, 0), (7, 9, 7))?, direction)),
        PieceKind::StairsRoom => Box::new(StairsRoom::new(depth, create((-2, 0, 0), (7, 11, 7))?, direction)),
        PieceKind::MonsterThrone => Box::new(MonsterThrone::new(depth, create((-2, 0, 0), (7, 8, 9))?, direction)),
        PieceKind::CastleEntrance => Box::new(CastleEntrance::new(depth, create((-5, -3, 0), (13, 14, 13))?, direction)),
        PieceKind::CastleSmallCorridor => Box::new(CastleSmallCorridor::new(depth, create((-1, 0, 0), (5, 7, 5))?, direction)),
        PieceKind::CastleSmallCorridorRightTurn => {
            Box::new(CastleSmallCorridorRightTurn::create(depth, random, create((-1, 0, 0), (5, 7, 5))?, direction))
        }
        PieceKind::CastleSmallCorridorLeftTurn => {
            Box::new(CastleSmallCorridorLeftTurn::create(depth, random, create((-1, 0, 0), (5, 7, 5))?, direction))
        }
        PieceKind::CastleCorridorStairs => Box::new(CastleCorridorStairs::new(depth, create((-1, -7, 0), (5, 14, 10))?, direction)),
        PieceKind::CastleCorridorTBalcony => Box::new(CastleCorridorTBalcony::new(depth, create((-3, 0, 0), (9, 7, 9))?, direction)),
        PieceKind::CastleSmallCorridorCrossing => {
            Box::new(CastleSmallCorridorCrossing::new(depth, create((-1, 0, 0), (5, 7, 5))?, direction))
        }
        PieceKind::CastleStalkRoom => Box::new(CastleStalkRoom::new(depth, create((-5, -3, 0), (13, 14, 13))?, direction)),
    })
}

/// `BridgeEndFiller.createPiece(builder, random, footX, footY, footZ, direction, genDepth)`.
#[allow(clippy::too_many_arguments)]
fn create_bridge_end_filler(
    builder: &StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    gen_depth: i32,
) -> Option<Box<dyn StructurePiece>> {
    let bounding_box = create_box(builder, (foot_x, foot_y, foot_z), (-1, -3, 0), (5, 10, 8), direction)?;
    Some(Box::new(BridgeEndFiller::new(gen_depth, random, bounding_box, direction)))
}

/// `NetherBridgePiece.updatePieceWeight(currentPieces)`.
fn update_piece_weight(current_pieces: &[PieceWeight]) -> i32 {
    let mut has_any_pieces = false;
    let mut total_weight = 0;
    for piece in current_pieces {
        if piece.max_place_count > 0 && piece.place_count < piece.max_place_count {
            has_any_pieces = true;
        }
        total_weight += piece.weight;
    }
    if has_any_pieces { total_weight } else { -1 }
}

/// `NetherBridgePiece.generatePiece(startPiece, currentPieces, builder, random, footX, footY, footZ, direction, depth)`
/// (`currentPieces` is the start piece's castle or bridge list).
#[allow(clippy::too_many_arguments)]
fn generate_piece(
    start: &mut StartPiece,
    is_castle: bool,
    builder: &StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
) -> Option<Box<dyn StructurePiece>> {
    let current_pieces = if is_castle { &mut start.available_castle_pieces } else { &mut start.available_bridge_pieces };
    let total_weight = update_piece_weight(current_pieces);
    let do_stuff = total_weight > 0 && depth <= MAX_DEPTH;
    let mut num_attempts = 0;
    while num_attempts < 5 && do_stuff {
        num_attempts += 1;
        let mut weight_selection = random.next_int_bounded(total_weight);
        for index in 0..current_pieces.len() {
            let piece = &current_pieces[index];
            weight_selection -= piece.weight;
            if weight_selection < 0 {
                if !piece.do_place(depth) || (Some(piece.kind) == start.previous_piece && !piece.allow_in_row) {
                    break;
                }
                let kind = piece.kind;
                if let Some(structure_piece) = find_and_create_bridge_piece_factory(kind, builder, random, foot_x, foot_y, foot_z, direction, depth) {
                    current_pieces[index].place_count += 1;
                    start.previous_piece = Some(kind);
                    if !current_pieces[index].is_valid() {
                        current_pieces.remove(index);
                    }
                    return Some(structure_piece);
                }
            }
        }
    }
    create_bridge_end_filler(builder, random, foot_x, foot_y, foot_z, direction, depth)
}

/// `NetherBridgePiece.generateAndAddPiece(startPiece, builder, random, footX, footY, footZ, direction, depth, isCastle)`.
#[allow(clippy::too_many_arguments)]
fn generate_and_add_piece(
    start: &mut StartPiece,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
    is_castle: bool,
) {
    if (foot_x.wrapping_sub(start.base.bounding_box.min_x())).wrapping_abs() <= 112
        && (foot_z.wrapping_sub(start.base.bounding_box.min_z())).wrapping_abs() <= 112
    {
        if let Some(new_piece) = generate_piece(start, is_castle, builder, random, foot_x, foot_y, foot_z, direction, depth + 1) {
            builder.add_piece(new_piece);
            start.pending_children.push(builder.len() - 1);
        }
    } else {
        // the filler is created (it draws its seed) but never added
        create_bridge_end_filler(builder, random, foot_x, foot_y, foot_z, direction, depth);
    }
}

/// `generateChildForward(startPiece, builder, random, xOff, yOff, isCastle)` of the piece described by `piece`.
fn generate_child_forward(
    piece: &PieceBase,
    start: &mut StartPiece,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    x_off: i32,
    y_off: i32,
    is_castle: bool,
) {
    let bb = &piece.bounding_box;
    let Some(orientation) = piece.orientation() else { return };
    let depth = piece.gen_depth;
    let (x, z) = match orientation {
        Direction::North => (bb.min_x() + x_off, bb.min_z() - 1),
        Direction::South => (bb.min_x() + x_off, bb.max_z() + 1),
        Direction::West => (bb.min_x() - 1, bb.min_z() + x_off),
        Direction::East => (bb.max_x() + 1, bb.min_z() + x_off),
        _ => return,
    };
    generate_and_add_piece(start, builder, random, x, bb.min_y() + y_off, z, orientation, depth, is_castle);
}

/// `generateChildLeft(startPiece, builder, random, yOff, zOff, isCastle)`.
fn generate_child_left(
    piece: &PieceBase,
    start: &mut StartPiece,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    y_off: i32,
    z_off: i32,
    is_castle: bool,
) {
    let bb = &piece.bounding_box;
    let Some(orientation) = piece.orientation() else { return };
    let depth = piece.gen_depth;
    let (x, z, direction) = match orientation {
        Direction::North | Direction::South => (bb.min_x() - 1, bb.min_z() + z_off, Direction::West),
        Direction::West | Direction::East => (bb.min_x() + z_off, bb.min_z() - 1, Direction::North),
        _ => return,
    };
    generate_and_add_piece(start, builder, random, x, bb.min_y() + y_off, z, direction, depth, is_castle);
}

/// `generateChildRight(startPiece, builder, random, yOff, zOff, isCastle)`.
fn generate_child_right(
    piece: &PieceBase,
    start: &mut StartPiece,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    y_off: i32,
    z_off: i32,
    is_castle: bool,
) {
    let bb = &piece.bounding_box;
    let Some(orientation) = piece.orientation() else { return };
    let depth = piece.gen_depth;
    let (x, z, direction) = match orientation {
        Direction::North | Direction::South => (bb.max_x() + 1, bb.min_z() + z_off, Direction::East),
        Direction::West | Direction::East => (bb.min_x() + z_off, bb.max_z() + 1, Direction::South),
        _ => return,
    };
    generate_and_add_piece(start, builder, random, x, bb.min_y() + y_off, z, direction, depth, is_castle);
}

/// The `(StartPiece)startPiece` cast of every `addChildren`.
fn start_of(start_piece: Option<&mut dyn StructurePiece>) -> &mut StartPiece {
    start_piece.and_then(|p| p.downcast_mut::<StartPiece>()).expect("the start piece of a nether fortress")
}

fn nether_bricks() -> BlockState {
    Block::NETHER_BRICKS.default_state()
}

fn air() -> BlockState {
    Block::AIR.default_state()
}

/// A `NETHER_BRICK_FENCE` state with the given sides.
fn fence(north: bool, east: bool, south: bool, west: bool) -> BlockState {
    Block::NETHER_BRICK_FENCE
        .default_state()
        .set(props::NORTH, north)
        .set(props::EAST, east)
        .set(props::SOUTH, south)
        .set(props::WEST, west)
}

fn ns_fence() -> BlockState {
    fence(true, false, true, false)
}

fn we_fence() -> BlockState {
    fence(false, true, false, true)
}

fn stairs(facing: Direction) -> BlockState {
    Block::NETHER_BRICK_STAIRS.default_state().set(props::HORIZONTAL_FACING, facing)
}

/// `generateBox(level, chunkBB, x0, y0, z0, x1, y1, z1, block, block, false)`, the only box form of the fortress.
trait FillBox: StructurePiece {
    fn fill(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, [x0, y0, z0, x1, y1, z1]: [i32; 6], state: BlockState) {
        self.generate_box(level, chunk_bb, x0, y0, z0, x1, y1, z1, state, state, false);
    }

    /// `fillColumnDown(level, NETHER_BRICKS, x, -1, z, chunkBB)`.
    fn foundation(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, x: i32, z: i32) {
        self.fill_column_down(level, nether_bricks(), x, -1, z, chunk_bb);
    }
}

impl<T: StructurePiece + ?Sized> FillBox for T {}

macro_rules! base_accessors {
    () => {
        fn base(&self) -> &PieceBase {
            &self.base
        }

        fn base_mut(&mut self) -> &mut PieceBase {
            &mut self.base
        }
    };
}

/// `postProcess(level, structureManager, generator, random, chunkBB, chunkPos, referencePos)` with only the used parameters named.
macro_rules! post_process {
    ($self:ident, $level:ident, $random:ident, $chunk_bb:ident, $body:block) => {
        fn post_process(
            &mut $self,
            $level: &mut dyn WorldGenLevel,
            _: &StructureManager,
            _: &GenEnv,
            $random: &mut dyn RandomSource,
            $chunk_bb: &BoundingBox,
            _: ChunkPos,
            _: BlockPos,
        ) $body
    };
}

/// A piece without own data (`addAdditionalSaveData` is empty): struct, piece type, constructors.
macro_rules! plain_piece {
    ($name:ident, $type_static:ident, $id:literal) => {
        pub struct $name {
            base: PieceBase,
        }

        #[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
        pub static $type_static: StructurePieceType =
            StructurePieceType { id: $id, load: |_, tag| Ok(Box::new($name { base: PieceBase::load(&$type_static, tag)? })) };

        impl $name {
            pub fn new(gen_depth: i32, bounding_box: BoundingBox, direction: Direction) -> $name {
                let mut base = PieceBase::new(&$type_static, gen_depth, bounding_box);
                base.set_orientation(Some(direction));
                $name { base }
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------------------------------
// BridgeCrossing / StartPiece
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(BridgeCrossing, NETHER_FORTRESS_BRIDGE_CROSSING, "nebcr");

/// `BridgeCrossing.addChildren`.
fn bridge_crossing_add_children(piece: &PieceBase, start: &mut StartPiece, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
    generate_child_forward(piece, start, builder, random, 8, 3, false);
    generate_child_left(piece, start, builder, random, 3, 8, false);
    generate_child_right(piece, start, builder, random, 3, 8, false);
}

/// `BridgeCrossing.postProcess` (the start piece inherits it).
fn bridge_crossing_post_process(piece: &dyn StructurePiece, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox) {
    let nb = nether_bricks();
    let air = air();
    piece.fill(level, chunk_bb, [7, 3, 0, 11, 4, 18], nb);
    piece.fill(level, chunk_bb, [0, 3, 7, 18, 4, 11], nb);
    piece.fill(level, chunk_bb, [8, 5, 0, 10, 7, 18], air);
    piece.fill(level, chunk_bb, [0, 5, 8, 18, 7, 10], air);
    piece.fill(level, chunk_bb, [7, 5, 0, 7, 5, 7], nb);
    piece.fill(level, chunk_bb, [7, 5, 11, 7, 5, 18], nb);
    piece.fill(level, chunk_bb, [11, 5, 0, 11, 5, 7], nb);
    piece.fill(level, chunk_bb, [11, 5, 11, 11, 5, 18], nb);
    piece.fill(level, chunk_bb, [0, 5, 7, 7, 5, 7], nb);
    piece.fill(level, chunk_bb, [11, 5, 7, 18, 5, 7], nb);
    piece.fill(level, chunk_bb, [0, 5, 11, 7, 5, 11], nb);
    piece.fill(level, chunk_bb, [11, 5, 11, 18, 5, 11], nb);
    piece.fill(level, chunk_bb, [7, 2, 0, 11, 2, 5], nb);
    piece.fill(level, chunk_bb, [7, 2, 13, 11, 2, 18], nb);
    piece.fill(level, chunk_bb, [7, 0, 0, 11, 1, 3], nb);
    piece.fill(level, chunk_bb, [7, 0, 15, 11, 1, 18], nb);

    for x in 7..=11 {
        for z in 0..=2 {
            piece.foundation(level, chunk_bb, x, z);
            piece.foundation(level, chunk_bb, x, 18 - z);
        }
    }

    piece.fill(level, chunk_bb, [0, 2, 7, 5, 2, 11], nb);
    piece.fill(level, chunk_bb, [13, 2, 7, 18, 2, 11], nb);
    piece.fill(level, chunk_bb, [0, 0, 7, 3, 1, 11], nb);
    piece.fill(level, chunk_bb, [15, 0, 7, 18, 1, 11], nb);

    for x in 0..=2 {
        for z in 7..=11 {
            piece.foundation(level, chunk_bb, x, z);
            piece.foundation(level, chunk_bb, 18 - x, z);
        }
    }
}

impl StructurePiece for BridgeCrossing {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        bridge_crossing_add_children(&self.base, start_of(start_piece), builder, random);
    }

    post_process!(self, level, _random, chunk_bb, {
        bridge_crossing_post_process(self, level, chunk_bb);
    });
}

/// `StartPiece`: a `BridgeCrossing` that owns the generation state of the fortress.
pub struct StartPiece {
    base: PieceBase,
    previous_piece: Option<PieceKind>,
    available_bridge_pieces: Vec<PieceWeight>,
    available_castle_pieces: Vec<PieceWeight>,
    /// `pendingChildren`: builder indices of the pieces whose `addChildren` is still to run.
    pub pending_children: Vec<usize>,
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static NETHER_FORTRESS_START: StructurePieceType =
    StructurePieceType { id: "nestart", load: |_, tag| Ok(Box::new(StartPiece::load(tag)?)) };

impl StartPiece {
    /// `StartPiece(random, west, north)`: the `BridgeCrossing(west, north, direction)` constructor, so a generated start
    /// piece is a `NeBCr` piece (only a saved `NeStart` loads as a `StartPiece` type).
    pub fn new(random: &mut dyn RandomSource, west: i32, north: i32) -> StartPiece {
        let direction = get_random_horizontal_direction(random);
        let mut base = PieceBase::new(&NETHER_FORTRESS_BRIDGE_CROSSING, 0, make_bounding_box(west, MAGIC_START_Y, north, direction, 19, 10, 19));
        base.set_orientation(Some(direction));
        StartPiece {
            base,
            previous_piece: None,
            available_bridge_pieces: BRIDGE_PIECE_WEIGHTS.to_vec(),
            available_castle_pieces: CASTLE_PIECE_WEIGHTS.to_vec(),
            pending_children: Vec::new(),
        }
    }

    pub fn load(tag: &CompoundTag) -> Result<StartPiece, String> {
        Ok(StartPiece {
            base: PieceBase::load(&NETHER_FORTRESS_START, tag)?,
            previous_piece: None,
            available_bridge_pieces: Vec::new(),
            available_castle_pieces: Vec::new(),
            pending_children: Vec::new(),
        })
    }
}

impl StructurePiece for StartPiece {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, _start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        // `startPiece` is this piece itself
        let piece = self.base.clone();
        bridge_crossing_add_children(&piece, self, builder, random);
    }

    post_process!(self, level, _random, chunk_bb, {
        bridge_crossing_post_process(self, level, chunk_bb);
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// BridgeEndFiller
// ---------------------------------------------------------------------------------------------------------------------

pub struct BridgeEndFiller {
    base: PieceBase,
    self_seed: i32,
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static NETHER_FORTRESS_BRIDGE_END_FILLER: StructurePieceType = StructurePieceType {
    id: "nebef",
    load: |_, tag| {
        Ok(Box::new(BridgeEndFiller { base: PieceBase::load(&NETHER_FORTRESS_BRIDGE_END_FILLER, tag)?, self_seed: tag.get_int_or("Seed", 0) }))
    },
};

impl BridgeEndFiller {
    pub fn new(gen_depth: i32, random: &mut dyn RandomSource, bounding_box: BoundingBox, direction: Direction) -> BridgeEndFiller {
        let mut base = PieceBase::new(&NETHER_FORTRESS_BRIDGE_END_FILLER, gen_depth, bounding_box);
        base.set_orientation(Some(direction));
        BridgeEndFiller { base, self_seed: random.next_int() }
    }
}

impl StructurePiece for BridgeEndFiller {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_int("Seed", self.self_seed);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        let mut self_random = SingleThreadedRandomSource::new(self.self_seed as i64);

        for x in 0..=4 {
            for y in 3..=4 {
                let z = self_random.next_int_bounded(8);
                self.fill(level, chunk_bb, [x, y, 0, x, y, z], nb);
            }
        }

        let z = self_random.next_int_bounded(8);
        self.fill(level, chunk_bb, [0, 5, 0, 0, 5, z], nb);
        let z = self_random.next_int_bounded(8);
        self.fill(level, chunk_bb, [4, 5, 0, 4, 5, z], nb);

        for x in 0..=4 {
            let z = self_random.next_int_bounded(5);
            self.fill(level, chunk_bb, [x, 2, 0, x, 2, z], nb);
        }

        for x in 0..=4 {
            for y in 0..=1 {
                let z = self_random.next_int_bounded(3);
                self.fill(level, chunk_bb, [x, y, 0, x, y, z], nb);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// BridgeStraight
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(BridgeStraight, NETHER_FORTRESS_BRIDGE_STRAIGHT, "nebs");

impl StructurePiece for BridgeStraight {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_forward(&self.base, start_of(start_piece), builder, random, 1, 3, false);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 3, 0, 4, 4, 18], nb);
        self.fill(level, chunk_bb, [1, 5, 0, 3, 7, 18], air());
        self.fill(level, chunk_bb, [0, 5, 0, 0, 5, 18], nb);
        self.fill(level, chunk_bb, [4, 5, 0, 4, 5, 18], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 4, 2, 5], nb);
        self.fill(level, chunk_bb, [0, 2, 13, 4, 2, 18], nb);
        self.fill(level, chunk_bb, [0, 0, 0, 4, 1, 3], nb);
        self.fill(level, chunk_bb, [0, 0, 15, 4, 1, 18], nb);

        for x in 0..=4 {
            for z in 0..=2 {
                self.foundation(level, chunk_bb, x, z);
                self.foundation(level, chunk_bb, x, 18 - z);
            }
        }

        let nse_fence = fence(true, true, true, false);
        let nsw_fence = fence(true, false, true, true);
        self.fill(level, chunk_bb, [0, 1, 1, 0, 4, 1], nse_fence);
        self.fill(level, chunk_bb, [0, 3, 4, 0, 4, 4], nse_fence);
        self.fill(level, chunk_bb, [0, 3, 14, 0, 4, 14], nse_fence);
        self.fill(level, chunk_bb, [0, 1, 17, 0, 4, 17], nse_fence);
        self.fill(level, chunk_bb, [4, 1, 1, 4, 4, 1], nsw_fence);
        self.fill(level, chunk_bb, [4, 3, 4, 4, 4, 4], nsw_fence);
        self.fill(level, chunk_bb, [4, 3, 14, 4, 4, 14], nsw_fence);
        self.fill(level, chunk_bb, [4, 1, 17, 4, 4, 17], nsw_fence);
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleCorridorStairsPiece
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(CastleCorridorStairs, NETHER_FORTRESS_CASTLE_CORRIDOR_STAIRS, "neccs");

impl StructurePiece for CastleCorridorStairs {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_forward(&self.base, start_of(start_piece), builder, random, 1, 0, true);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        let stairs = stairs(Direction::South);
        let ns_fence = ns_fence();

        for step in 0..=9 {
            let floor = 1.max(7 - step);
            let roof = (floor + 5).max(14 - step).min(13);
            let z = step;
            self.fill(level, chunk_bb, [0, 0, step, 4, floor, step], nb);
            self.fill(level, chunk_bb, [1, floor + 1, step, 3, roof - 1, step], air());
            if step <= 6 {
                self.place_block(level, stairs, 1, floor + 1, step, chunk_bb);
                self.place_block(level, stairs, 2, floor + 1, step, chunk_bb);
                self.place_block(level, stairs, 3, floor + 1, step, chunk_bb);
            }

            self.fill(level, chunk_bb, [0, roof, step, 4, roof, step], nb);
            self.fill(level, chunk_bb, [0, floor + 1, step, 0, roof - 1, step], nb);
            self.fill(level, chunk_bb, [4, floor + 1, step, 4, roof - 1, step], nb);
            if (step & 1) == 0 {
                self.fill(level, chunk_bb, [0, floor + 2, step, 0, floor + 3, step], ns_fence);
                self.fill(level, chunk_bb, [4, floor + 2, step, 4, floor + 3, step], ns_fence);
            }

            for x in 0..=4 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleCorridorTBalconyPiece
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(CastleCorridorTBalcony, NETHER_FORTRESS_CASTLE_CORRIDOR_T_BALCONY, "nectb");

impl StructurePiece for CastleCorridorTBalcony {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let start = start_of(start_piece);
        let mut z_off = 1;
        let orientation = self.get_orientation();
        if orientation == Some(Direction::West) || orientation == Some(Direction::North) {
            z_off = 5;
        }

        let is_castle = random.next_int_bounded(8) > 0;
        generate_child_left(&self.base, start, builder, random, 0, z_off, is_castle);
        let is_castle = random.next_int_bounded(8) > 0;
        generate_child_right(&self.base, start, builder, random, 0, z_off, is_castle);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        let ns_fence = ns_fence();
        let we_fence = we_fence();
        self.fill(level, chunk_bb, [0, 0, 0, 8, 1, 8], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 8, 5, 8], air());
        self.fill(level, chunk_bb, [0, 6, 0, 8, 6, 5], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 2, 5, 0], nb);
        self.fill(level, chunk_bb, [6, 2, 0, 8, 5, 0], nb);
        self.fill(level, chunk_bb, [1, 3, 0, 1, 4, 0], we_fence);
        self.fill(level, chunk_bb, [7, 3, 0, 7, 4, 0], we_fence);
        self.fill(level, chunk_bb, [0, 2, 4, 8, 2, 8], nb);
        self.fill(level, chunk_bb, [1, 1, 4, 2, 2, 4], air());
        self.fill(level, chunk_bb, [6, 1, 4, 7, 2, 4], air());
        self.fill(level, chunk_bb, [1, 3, 8, 7, 3, 8], we_fence);
        self.place_block(level, fence(false, true, true, false), 0, 3, 8, chunk_bb);
        self.place_block(level, fence(false, false, true, true), 8, 3, 8, chunk_bb);
        self.fill(level, chunk_bb, [0, 3, 6, 0, 3, 7], ns_fence);
        self.fill(level, chunk_bb, [8, 3, 6, 8, 3, 7], ns_fence);
        self.fill(level, chunk_bb, [0, 3, 4, 0, 5, 5], nb);
        self.fill(level, chunk_bb, [8, 3, 4, 8, 5, 5], nb);
        self.fill(level, chunk_bb, [1, 3, 5, 2, 5, 5], nb);
        self.fill(level, chunk_bb, [6, 3, 5, 7, 5, 5], nb);
        self.fill(level, chunk_bb, [1, 4, 5, 1, 5, 5], we_fence);
        self.fill(level, chunk_bb, [7, 4, 5, 7, 5, 5], we_fence);

        for z in 0..=5 {
            for x in 0..=8 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleEntrance
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(CastleEntrance, NETHER_FORTRESS_CASTLE_ENTRANCE, "nece");

impl StructurePiece for CastleEntrance {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_forward(&self.base, start_of(start_piece), builder, random, 5, 3, true);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        let air = air();
        self.fill(level, chunk_bb, [0, 3, 0, 12, 4, 12], nb);
        self.fill(level, chunk_bb, [0, 5, 0, 12, 13, 12], air);
        self.fill(level, chunk_bb, [0, 5, 0, 1, 12, 12], nb);
        self.fill(level, chunk_bb, [11, 5, 0, 12, 12, 12], nb);
        self.fill(level, chunk_bb, [2, 5, 11, 4, 12, 12], nb);
        self.fill(level, chunk_bb, [8, 5, 11, 10, 12, 12], nb);
        self.fill(level, chunk_bb, [5, 9, 11, 7, 12, 12], nb);
        self.fill(level, chunk_bb, [2, 5, 0, 4, 12, 1], nb);
        self.fill(level, chunk_bb, [8, 5, 0, 10, 12, 1], nb);
        self.fill(level, chunk_bb, [5, 9, 0, 7, 12, 1], nb);
        self.fill(level, chunk_bb, [2, 11, 2, 10, 12, 10], nb);
        self.fill(level, chunk_bb, [5, 8, 0, 7, 8, 0], Block::NETHER_BRICK_FENCE.default_state());
        let we_fence = we_fence();
        let ns_fence = ns_fence();

        let mut i = 1;
        while i <= 11 {
            self.fill(level, chunk_bb, [i, 10, 0, i, 11, 0], we_fence);
            self.fill(level, chunk_bb, [i, 10, 12, i, 11, 12], we_fence);
            self.fill(level, chunk_bb, [0, 10, i, 0, 11, i], ns_fence);
            self.fill(level, chunk_bb, [12, 10, i, 12, 11, i], ns_fence);
            self.place_block(level, nb, i, 13, 0, chunk_bb);
            self.place_block(level, nb, i, 13, 12, chunk_bb);
            self.place_block(level, nb, 0, 13, i, chunk_bb);
            self.place_block(level, nb, 12, 13, i, chunk_bb);
            if i != 11 {
                self.place_block(level, we_fence, i + 1, 13, 0, chunk_bb);
                self.place_block(level, we_fence, i + 1, 13, 12, chunk_bb);
                self.place_block(level, ns_fence, 0, 13, i + 1, chunk_bb);
                self.place_block(level, ns_fence, 12, 13, i + 1, chunk_bb);
            }
            i += 2;
        }

        self.place_block(level, fence(true, true, false, false), 0, 13, 0, chunk_bb);
        self.place_block(level, fence(false, true, true, false), 0, 13, 12, chunk_bb);
        self.place_block(level, fence(false, false, true, true), 12, 13, 12, chunk_bb);
        self.place_block(level, fence(true, false, false, true), 12, 13, 0, chunk_bb);

        let mut z = 3;
        while z <= 9 {
            let nsw_fence = fence(true, false, true, true);
            let nse_fence = fence(true, true, true, false);
            self.fill(level, chunk_bb, [1, 7, z, 1, 8, z], nsw_fence);
            self.fill(level, chunk_bb, [11, 7, z, 11, 8, z], nse_fence);
            z += 2;
        }

        self.fill(level, chunk_bb, [4, 2, 0, 8, 2, 12], nb);
        self.fill(level, chunk_bb, [0, 2, 4, 12, 2, 8], nb);
        self.fill(level, chunk_bb, [4, 0, 0, 8, 1, 3], nb);
        self.fill(level, chunk_bb, [4, 0, 9, 8, 1, 12], nb);
        self.fill(level, chunk_bb, [0, 0, 4, 3, 1, 8], nb);
        self.fill(level, chunk_bb, [9, 0, 4, 12, 1, 8], nb);

        for x in 4..=8 {
            for z in 0..=2 {
                self.foundation(level, chunk_bb, x, z);
                self.foundation(level, chunk_bb, x, 12 - z);
            }
        }

        for x in 0..=2 {
            for z in 4..=8 {
                self.foundation(level, chunk_bb, x, z);
                self.foundation(level, chunk_bb, 12 - x, z);
            }
        }

        self.fill(level, chunk_bb, [5, 5, 5, 7, 5, 7], nb);
        self.fill(level, chunk_bb, [6, 1, 6, 6, 4, 6], air);
        self.place_block(level, nb, 6, 0, 6, chunk_bb);
        self.place_block(level, Block::LAVA.default_state(), 6, 5, 6, chunk_bb);
        let pos = self.get_world_pos(6, 5, 6);
        if chunk_bb.is_inside(&pos) {
            level.schedule_fluid_tick(pos, Fluid::LAVA, 0, TickPriority::Normal);
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleSmallCorridorCrossingPiece
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(CastleSmallCorridorCrossing, NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_CROSSING, "nescsc");

impl StructurePiece for CastleSmallCorridorCrossing {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let start = start_of(start_piece);
        generate_child_forward(&self.base, start, builder, random, 1, 0, true);
        generate_child_left(&self.base, start, builder, random, 0, 1, true);
        generate_child_right(&self.base, start, builder, random, 0, 1, true);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 0, 0, 4, 1, 4], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 4, 5, 4], air());
        self.fill(level, chunk_bb, [0, 2, 0, 0, 5, 0], nb);
        self.fill(level, chunk_bb, [4, 2, 0, 4, 5, 0], nb);
        self.fill(level, chunk_bb, [0, 2, 4, 0, 5, 4], nb);
        self.fill(level, chunk_bb, [4, 2, 4, 4, 5, 4], nb);
        self.fill(level, chunk_bb, [0, 6, 0, 4, 6, 4], nb);

        for x in 0..=4 {
            for z in 0..=4 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleSmallCorridorLeftTurnPiece / CastleSmallCorridorRightTurnPiece
// ---------------------------------------------------------------------------------------------------------------------

pub struct CastleSmallCorridorLeftTurn {
    base: PieceBase,
    is_needing_chest: bool,
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_LEFT_TURN: StructurePieceType = StructurePieceType {
    id: "nesclt",
    load: |_, tag| {
        Ok(Box::new(CastleSmallCorridorLeftTurn {
            base: PieceBase::load(&NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_LEFT_TURN, tag)?,
            is_needing_chest: tag.get_boolean_or("Chest", false),
        }))
    },
};

impl CastleSmallCorridorLeftTurn {
    pub fn create(gen_depth: i32, random: &mut dyn RandomSource, bounding_box: BoundingBox, direction: Direction) -> CastleSmallCorridorLeftTurn {
        let mut base = PieceBase::new(&NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_LEFT_TURN, gen_depth, bounding_box);
        base.set_orientation(Some(direction));
        CastleSmallCorridorLeftTurn { base, is_needing_chest: random.next_int_bounded(3) == 0 }
    }
}

impl StructurePiece for CastleSmallCorridorLeftTurn {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_boolean("Chest", self.is_needing_chest);
    }

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_left(&self.base, start_of(start_piece), builder, random, 0, 1, true);
    }

    post_process!(self, level, random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 0, 0, 4, 1, 4], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 4, 5, 4], air());
        let we_fence = we_fence();
        let ns_fence = ns_fence();
        self.fill(level, chunk_bb, [4, 2, 0, 4, 5, 4], nb);
        self.fill(level, chunk_bb, [4, 3, 1, 4, 4, 1], ns_fence);
        self.fill(level, chunk_bb, [4, 3, 3, 4, 4, 3], ns_fence);
        self.fill(level, chunk_bb, [0, 2, 0, 0, 5, 0], nb);
        self.fill(level, chunk_bb, [0, 2, 4, 3, 5, 4], nb);
        self.fill(level, chunk_bb, [1, 3, 4, 1, 4, 4], we_fence);
        self.fill(level, chunk_bb, [3, 3, 4, 3, 4, 4], we_fence);
        if self.is_needing_chest && chunk_bb.is_inside(&self.get_world_pos(3, 2, 3)) {
            self.is_needing_chest = false;
            self.create_chest(level, chunk_bb, random, 3, 2, 3, CHEST_LOOT_TABLE);
        }

        self.fill(level, chunk_bb, [0, 6, 0, 4, 6, 4], nb);

        for x in 0..=4 {
            for z in 0..=4 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

pub struct CastleSmallCorridorRightTurn {
    base: PieceBase,
    is_needing_chest: bool,
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_RIGHT_TURN: StructurePieceType = StructurePieceType {
    id: "nescrt",
    load: |_, tag| {
        Ok(Box::new(CastleSmallCorridorRightTurn {
            base: PieceBase::load(&NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_RIGHT_TURN, tag)?,
            is_needing_chest: tag.get_boolean_or("Chest", false),
        }))
    },
};

impl CastleSmallCorridorRightTurn {
    pub fn create(gen_depth: i32, random: &mut dyn RandomSource, bounding_box: BoundingBox, direction: Direction) -> CastleSmallCorridorRightTurn {
        let mut base = PieceBase::new(&NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR_RIGHT_TURN, gen_depth, bounding_box);
        base.set_orientation(Some(direction));
        CastleSmallCorridorRightTurn { base, is_needing_chest: random.next_int_bounded(3) == 0 }
    }
}

impl StructurePiece for CastleSmallCorridorRightTurn {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_boolean("Chest", self.is_needing_chest);
    }

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_right(&self.base, start_of(start_piece), builder, random, 0, 1, true);
    }

    post_process!(self, level, random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 0, 0, 4, 1, 4], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 4, 5, 4], air());
        let we_fence = we_fence();
        let ns_fence = ns_fence();
        self.fill(level, chunk_bb, [0, 2, 0, 0, 5, 4], nb);
        self.fill(level, chunk_bb, [0, 3, 1, 0, 4, 1], ns_fence);
        self.fill(level, chunk_bb, [0, 3, 3, 0, 4, 3], ns_fence);
        self.fill(level, chunk_bb, [4, 2, 0, 4, 5, 0], nb);
        self.fill(level, chunk_bb, [1, 2, 4, 4, 5, 4], nb);
        self.fill(level, chunk_bb, [1, 3, 4, 1, 4, 4], we_fence);
        self.fill(level, chunk_bb, [3, 3, 4, 3, 4, 4], we_fence);
        if self.is_needing_chest && chunk_bb.is_inside(&self.get_world_pos(1, 2, 3)) {
            self.is_needing_chest = false;
            self.create_chest(level, chunk_bb, random, 1, 2, 3, CHEST_LOOT_TABLE);
        }

        self.fill(level, chunk_bb, [0, 6, 0, 4, 6, 4], nb);

        for x in 0..=4 {
            for z in 0..=4 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleSmallCorridorPiece
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(CastleSmallCorridor, NETHER_FORTRESS_CASTLE_SMALL_CORRIDOR, "nesc");

impl StructurePiece for CastleSmallCorridor {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_forward(&self.base, start_of(start_piece), builder, random, 1, 0, true);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 0, 0, 4, 1, 4], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 4, 5, 4], air());
        let ns_fence = ns_fence();
        self.fill(level, chunk_bb, [0, 2, 0, 0, 5, 4], nb);
        self.fill(level, chunk_bb, [4, 2, 0, 4, 5, 4], nb);
        self.fill(level, chunk_bb, [0, 3, 1, 0, 4, 1], ns_fence);
        self.fill(level, chunk_bb, [0, 3, 3, 0, 4, 3], ns_fence);
        self.fill(level, chunk_bb, [4, 3, 1, 4, 4, 1], ns_fence);
        self.fill(level, chunk_bb, [4, 3, 3, 4, 4, 3], ns_fence);
        self.fill(level, chunk_bb, [0, 6, 0, 4, 6, 4], nb);

        for x in 0..=4 {
            for z in 0..=4 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// CastleStalkRoom
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(CastleStalkRoom, NETHER_FORTRESS_CASTLE_STALK_ROOM, "necsr");

impl StructurePiece for CastleStalkRoom {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let start = start_of(start_piece);
        generate_child_forward(&self.base, start, builder, random, 5, 3, true);
        generate_child_forward(&self.base, start, builder, random, 5, 11, true);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        let air = air();
        self.fill(level, chunk_bb, [0, 3, 0, 12, 4, 12], nb);
        self.fill(level, chunk_bb, [0, 5, 0, 12, 13, 12], air);
        self.fill(level, chunk_bb, [0, 5, 0, 1, 12, 12], nb);
        self.fill(level, chunk_bb, [11, 5, 0, 12, 12, 12], nb);
        self.fill(level, chunk_bb, [2, 5, 11, 4, 12, 12], nb);
        self.fill(level, chunk_bb, [8, 5, 11, 10, 12, 12], nb);
        self.fill(level, chunk_bb, [5, 9, 11, 7, 12, 12], nb);
        self.fill(level, chunk_bb, [2, 5, 0, 4, 12, 1], nb);
        self.fill(level, chunk_bb, [8, 5, 0, 10, 12, 1], nb);
        self.fill(level, chunk_bb, [5, 9, 0, 7, 12, 1], nb);
        self.fill(level, chunk_bb, [2, 11, 2, 10, 12, 10], nb);
        let we_fence = we_fence();
        let ns_fence = ns_fence();
        let nsw_fence = fence(true, false, true, true);
        let nse_fence = fence(true, true, true, false);

        let mut i = 1;
        while i <= 11 {
            self.fill(level, chunk_bb, [i, 10, 0, i, 11, 0], we_fence);
            self.fill(level, chunk_bb, [i, 10, 12, i, 11, 12], we_fence);
            self.fill(level, chunk_bb, [0, 10, i, 0, 11, i], ns_fence);
            self.fill(level, chunk_bb, [12, 10, i, 12, 11, i], ns_fence);
            self.place_block(level, nb, i, 13, 0, chunk_bb);
            self.place_block(level, nb, i, 13, 12, chunk_bb);
            self.place_block(level, nb, 0, 13, i, chunk_bb);
            self.place_block(level, nb, 12, 13, i, chunk_bb);
            if i != 11 {
                self.place_block(level, we_fence, i + 1, 13, 0, chunk_bb);
                self.place_block(level, we_fence, i + 1, 13, 12, chunk_bb);
                self.place_block(level, ns_fence, 0, 13, i + 1, chunk_bb);
                self.place_block(level, ns_fence, 12, 13, i + 1, chunk_bb);
            }
            i += 2;
        }

        self.place_block(level, fence(true, true, false, false), 0, 13, 0, chunk_bb);
        self.place_block(level, fence(false, true, true, false), 0, 13, 12, chunk_bb);
        self.place_block(level, fence(false, false, true, true), 12, 13, 12, chunk_bb);
        self.place_block(level, fence(true, false, false, true), 12, 13, 0, chunk_bb);

        let mut z = 3;
        while z <= 9 {
            self.fill(level, chunk_bb, [1, 7, z, 1, 8, z], nsw_fence);
            self.fill(level, chunk_bb, [11, 7, z, 11, 8, z], nse_fence);
            z += 2;
        }

        let stairs = stairs(Direction::North);

        for ix in 0..=6 {
            let z = ix + 4;

            for x in 5..=7 {
                self.place_block(level, stairs, x, 5 + ix, z, chunk_bb);
            }

            if (5..=8).contains(&z) {
                self.fill(level, chunk_bb, [5, 5, z, 7, ix + 4, z], nb);
            } else if (9..=10).contains(&z) {
                self.fill(level, chunk_bb, [5, 8, z, 7, ix + 4, z], nb);
            }

            if ix >= 1 {
                self.fill(level, chunk_bb, [5, 6 + ix, z, 7, 9 + ix, z], air);
            }
        }

        for x in 5..=7 {
            self.place_block(level, stairs, x, 12, 11, chunk_bb);
        }

        self.fill(level, chunk_bb, [5, 6, 7, 5, 7, 7], nse_fence);
        self.fill(level, chunk_bb, [7, 6, 7, 7, 7, 7], nsw_fence);
        self.fill(level, chunk_bb, [5, 13, 12, 7, 13, 12], air);
        self.fill(level, chunk_bb, [2, 5, 2, 3, 5, 3], nb);
        self.fill(level, chunk_bb, [2, 5, 9, 3, 5, 10], nb);
        self.fill(level, chunk_bb, [2, 5, 4, 2, 5, 8], nb);
        self.fill(level, chunk_bb, [9, 5, 2, 10, 5, 3], nb);
        self.fill(level, chunk_bb, [9, 5, 9, 10, 5, 10], nb);
        self.fill(level, chunk_bb, [10, 5, 4, 10, 5, 8], nb);
        let east_stairs = self::stairs(Direction::East);
        let west_stairs = self::stairs(Direction::West);
        self.place_block(level, west_stairs, 4, 5, 2, chunk_bb);
        self.place_block(level, west_stairs, 4, 5, 3, chunk_bb);
        self.place_block(level, west_stairs, 4, 5, 9, chunk_bb);
        self.place_block(level, west_stairs, 4, 5, 10, chunk_bb);
        self.place_block(level, east_stairs, 8, 5, 2, chunk_bb);
        self.place_block(level, east_stairs, 8, 5, 3, chunk_bb);
        self.place_block(level, east_stairs, 8, 5, 9, chunk_bb);
        self.place_block(level, east_stairs, 8, 5, 10, chunk_bb);
        let soul_sand = Block::SOUL_SAND.default_state();
        let nether_wart = Block::NETHER_WART.default_state();
        self.fill(level, chunk_bb, [3, 4, 4, 4, 4, 8], soul_sand);
        self.fill(level, chunk_bb, [8, 4, 4, 9, 4, 8], soul_sand);
        self.fill(level, chunk_bb, [3, 5, 4, 4, 5, 8], nether_wart);
        self.fill(level, chunk_bb, [8, 5, 4, 9, 5, 8], nether_wart);
        self.fill(level, chunk_bb, [4, 2, 0, 8, 2, 12], nb);
        self.fill(level, chunk_bb, [0, 2, 4, 12, 2, 8], nb);
        self.fill(level, chunk_bb, [4, 0, 0, 8, 1, 3], nb);
        self.fill(level, chunk_bb, [4, 0, 9, 8, 1, 12], nb);
        self.fill(level, chunk_bb, [0, 0, 4, 3, 1, 8], nb);
        self.fill(level, chunk_bb, [9, 0, 4, 12, 1, 8], nb);

        for x in 4..=8 {
            for z in 0..=2 {
                self.foundation(level, chunk_bb, x, z);
                self.foundation(level, chunk_bb, x, 12 - z);
            }
        }

        for x in 0..=2 {
            for z in 4..=8 {
                self.foundation(level, chunk_bb, x, z);
                self.foundation(level, chunk_bb, 12 - x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// MonsterThrone
// ---------------------------------------------------------------------------------------------------------------------

pub struct MonsterThrone {
    base: PieceBase,
    has_placed_spawner: bool,
}

#[linkme::distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static NETHER_FORTRESS_MONSTER_THRONE: StructurePieceType = StructurePieceType {
    id: "nemt",
    load: |_, tag| {
        Ok(Box::new(MonsterThrone {
            base: PieceBase::load(&NETHER_FORTRESS_MONSTER_THRONE, tag)?,
            has_placed_spawner: tag.get_boolean_or("Mob", false),
        }))
    },
};

impl MonsterThrone {
    pub fn new(gen_depth: i32, bounding_box: BoundingBox, direction: Direction) -> MonsterThrone {
        let mut base = PieceBase::new(&NETHER_FORTRESS_MONSTER_THRONE, gen_depth, bounding_box);
        base.set_orientation(Some(direction));
        MonsterThrone { base, has_placed_spawner: false }
    }
}

impl StructurePiece for MonsterThrone {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_boolean("Mob", self.has_placed_spawner);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 2, 0, 6, 7, 7], air());
        self.fill(level, chunk_bb, [1, 0, 0, 5, 1, 7], nb);
        self.fill(level, chunk_bb, [1, 2, 1, 5, 2, 7], nb);
        self.fill(level, chunk_bb, [1, 3, 2, 5, 3, 7], nb);
        self.fill(level, chunk_bb, [1, 4, 3, 5, 4, 7], nb);
        self.fill(level, chunk_bb, [1, 2, 0, 1, 4, 2], nb);
        self.fill(level, chunk_bb, [5, 2, 0, 5, 4, 2], nb);
        self.fill(level, chunk_bb, [1, 5, 2, 1, 5, 3], nb);
        self.fill(level, chunk_bb, [5, 5, 2, 5, 5, 3], nb);
        self.fill(level, chunk_bb, [0, 5, 3, 0, 5, 8], nb);
        self.fill(level, chunk_bb, [6, 5, 3, 6, 5, 8], nb);
        self.fill(level, chunk_bb, [1, 5, 8, 5, 5, 8], nb);
        let we_fence = we_fence();
        let ns_fence = ns_fence();
        self.place_block(level, fence(false, false, false, true), 1, 6, 3, chunk_bb);
        self.place_block(level, fence(false, true, false, false), 5, 6, 3, chunk_bb);
        self.place_block(level, fence(true, true, false, false), 0, 6, 3, chunk_bb);
        self.place_block(level, fence(true, false, false, true), 6, 6, 3, chunk_bb);
        self.fill(level, chunk_bb, [0, 6, 4, 0, 6, 7], ns_fence);
        self.fill(level, chunk_bb, [6, 6, 4, 6, 6, 7], ns_fence);
        self.place_block(level, fence(false, true, true, false), 0, 6, 8, chunk_bb);
        self.place_block(level, fence(false, false, true, true), 6, 6, 8, chunk_bb);
        self.fill(level, chunk_bb, [1, 6, 8, 5, 6, 8], we_fence);
        self.place_block(level, fence(false, true, false, false), 1, 7, 8, chunk_bb);
        self.fill(level, chunk_bb, [2, 7, 8, 4, 7, 8], we_fence);
        self.place_block(level, fence(false, false, false, true), 5, 7, 8, chunk_bb);
        self.place_block(level, fence(false, true, false, false), 2, 8, 8, chunk_bb);
        self.place_block(level, we_fence, 3, 8, 8, chunk_bb);
        self.place_block(level, fence(false, false, false, true), 4, 8, 8, chunk_bb);
        if !self.has_placed_spawner {
            let pos = self.get_world_pos(3, 5, 5);
            if chunk_bb.is_inside(&pos) {
                self.has_placed_spawner = true;
                level.set_block_flags(pos, Block::SPAWNER.default_state(), 2);
                // `setEntityId` draws nothing from the random (the spawner has no potentials)
                if level.get_block_entity_nbt(pos).is_some() {
                    level.set_block_entity_nbt(spawner_tag(pos, "minecraft:blaze"));
                }
            }
        }

        for x in 0..=6 {
            for z in 0..=6 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// RoomCrossing
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(RoomCrossing, NETHER_FORTRESS_ROOM_CROSSING, "nerc");

impl StructurePiece for RoomCrossing {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let start = start_of(start_piece);
        generate_child_forward(&self.base, start, builder, random, 2, 0, false);
        generate_child_left(&self.base, start, builder, random, 0, 2, false);
        generate_child_right(&self.base, start, builder, random, 0, 2, false);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 0, 0, 6, 1, 6], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 6, 7, 6], air());
        self.fill(level, chunk_bb, [0, 2, 0, 1, 6, 0], nb);
        self.fill(level, chunk_bb, [0, 2, 6, 1, 6, 6], nb);
        self.fill(level, chunk_bb, [5, 2, 0, 6, 6, 0], nb);
        self.fill(level, chunk_bb, [5, 2, 6, 6, 6, 6], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 0, 6, 1], nb);
        self.fill(level, chunk_bb, [0, 2, 5, 0, 6, 6], nb);
        self.fill(level, chunk_bb, [6, 2, 0, 6, 6, 1], nb);
        self.fill(level, chunk_bb, [6, 2, 5, 6, 6, 6], nb);
        let we_fence = we_fence();
        let ns_fence = ns_fence();
        self.fill(level, chunk_bb, [2, 6, 0, 4, 6, 0], nb);
        self.fill(level, chunk_bb, [2, 5, 0, 4, 5, 0], we_fence);
        self.fill(level, chunk_bb, [2, 6, 6, 4, 6, 6], nb);
        self.fill(level, chunk_bb, [2, 5, 6, 4, 5, 6], we_fence);
        self.fill(level, chunk_bb, [0, 6, 2, 0, 6, 4], nb);
        self.fill(level, chunk_bb, [0, 5, 2, 0, 5, 4], ns_fence);
        self.fill(level, chunk_bb, [6, 6, 2, 6, 6, 4], nb);
        self.fill(level, chunk_bb, [6, 5, 2, 6, 5, 4], ns_fence);

        for x in 0..=6 {
            for z in 0..=6 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// StairsRoom
// ---------------------------------------------------------------------------------------------------------------------

plain_piece!(StairsRoom, NETHER_FORTRESS_STAIRS_ROOM, "nesr");

impl StructurePiece for StairsRoom {
    base_accessors!();

    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        generate_child_right(&self.base, start_of(start_piece), builder, random, 6, 2, false);
    }

    post_process!(self, level, _random, chunk_bb, {
        let nb = nether_bricks();
        self.fill(level, chunk_bb, [0, 0, 0, 6, 1, 6], nb);
        self.fill(level, chunk_bb, [0, 2, 0, 6, 10, 6], air());
        self.fill(level, chunk_bb, [0, 2, 0, 1, 8, 0], nb);
        self.fill(level, chunk_bb, [5, 2, 0, 6, 8, 0], nb);
        self.fill(level, chunk_bb, [0, 2, 1, 0, 8, 6], nb);
        self.fill(level, chunk_bb, [6, 2, 1, 6, 8, 6], nb);
        self.fill(level, chunk_bb, [1, 2, 6, 5, 8, 6], nb);
        let we_fence = we_fence();
        let ns_fence = ns_fence();
        self.fill(level, chunk_bb, [0, 3, 2, 0, 5, 4], ns_fence);
        self.fill(level, chunk_bb, [6, 3, 2, 6, 5, 2], ns_fence);
        self.fill(level, chunk_bb, [6, 3, 4, 6, 5, 4], ns_fence);
        self.place_block(level, nb, 5, 2, 5, chunk_bb);
        self.fill(level, chunk_bb, [4, 2, 5, 4, 3, 5], nb);
        self.fill(level, chunk_bb, [3, 2, 5, 3, 4, 5], nb);
        self.fill(level, chunk_bb, [2, 2, 5, 2, 5, 5], nb);
        self.fill(level, chunk_bb, [1, 2, 5, 1, 6, 5], nb);
        self.fill(level, chunk_bb, [1, 7, 1, 5, 7, 4], nb);
        self.fill(level, chunk_bb, [6, 8, 2, 6, 8, 4], air());
        self.fill(level, chunk_bb, [2, 6, 0, 4, 8, 0], nb);
        self.fill(level, chunk_bb, [2, 5, 0, 4, 5, 0], we_fence);

        for x in 0..=6 {
            for z in 0..=6 {
                self.foundation(level, chunk_bb, x, z);
            }
        }
    });
}
