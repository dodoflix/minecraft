//! `pieces/PiecesContainer` and `pieces/StructurePiecesBuilder`.

use super::piece::{StructurePiece, StructurePieceSerializationContext, StructurePieceType, create_bounding_box, find_collision_piece};
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_nbt::{CompoundTag, ListTag, Tag};

/// `PiecesContainer`.
pub struct PiecesContainer {
    pub pieces: Vec<Box<dyn StructurePiece>>,
}

impl PiecesContainer {
    pub fn new(pieces: Vec<Box<dyn StructurePiece>>) -> PiecesContainer {
        PiecesContainer { pieces }
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// `isInsidePiece(pos)`.
    pub fn is_inside_piece(&self, start_pos: BlockPos) -> bool {
        self.pieces.iter().any(|p| p.base().bounding_box.is_inside(&start_pos))
    }

    /// `save(context)`: the `Children` list.
    pub fn save(&self, context: &StructurePieceSerializationContext<'_>) -> ListTag {
        let mut children = ListTag::new();
        for piece in &self.pieces {
            children.push(Tag::Compound(piece.create_tag(context)));
        }
        children
    }

    /// `load(children, context)`: unknown ids and pieces whose loader fails are logged and skipped.
    pub fn load(children: &ListTag, context: &StructurePieceSerializationContext<'_>) -> PiecesContainer {
        let mut pieces = Vec::new();
        for child in children.iter() {
            let empty = CompoundTag::new();
            let piece_tag = child.as_compound().unwrap_or(&empty);
            let old_id = piece_tag.get_string_or("id", "").to_lowercase();
            let (namespace, path) = match old_id.split_once(':') {
                Some((n, p)) => (n.to_owned(), p.to_owned()),
                None => ("minecraft".to_owned(), old_id.clone()),
            };
            // `RENAMES`: legacy ids that became jigsaw pieces.
            let (namespace, path) = if namespace == "minecraft" && matches!(path.as_str(), "nvi" | "pcp" | "bastionremnant" | "runtime") {
                ("minecraft".to_owned(), "jigsaw".to_owned())
            } else {
                (namespace, path)
            };
            match StructurePieceType::by_key(&namespace, &path) {
                None => log::error!("Unknown structure piece id: {namespace}:{path}"),
                Some(piece_type) => match (piece_type.load)(context, piece_tag) {
                    Ok(piece) => pieces.push(piece),
                    Err(e) => log::error!("Exception loading structure piece with id {namespace}:{path}: {e}"),
                },
            }
        }
        PiecesContainer { pieces }
    }

    /// `calculateBoundingBox()`.
    pub fn calculate_bounding_box(&self) -> BoundingBox {
        create_bounding_box(self.pieces.iter().map(|p| &**p))
    }
}

/// The stand-in of a piece that is taken out of the builder while its children are generated
/// ([`StructurePiecesBuilder::add_piece_and_children`]): same type, box, orientation and depth, so collision checks
/// (`findCollisionPiece`, bounding boxes, gen depth) see it exactly as vanilla sees the piece.
struct TakenOutPiece {
    base: super::piece::PieceBase,
}

impl StructurePiece for TakenOutPiece {
    fn base(&self) -> &super::piece::PieceBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut super::piece::PieceBase {
        &mut self.base
    }
    fn add_additional_save_data(&self, _: &StructurePieceSerializationContext<'_>, _: &mut CompoundTag) {}
    fn post_process(
        &mut self,
        _: &mut dyn crate::world::level::levelgen::WorldGenLevel,
        _: &super::manager::StructureManager,
        _: &crate::world::level::levelgen::GenEnv,
        _: &mut dyn RandomSource,
        _: &BoundingBox,
        _: mc_core::pos::ChunkPos,
        _: BlockPos,
    ) {
    }
}

/// `StructurePiecesBuilder`.
#[derive(Default)]
pub struct StructurePiecesBuilder {
    pieces: Vec<Box<dyn StructurePiece>>,
}

impl StructurePiecesBuilder {
    pub fn new() -> StructurePiecesBuilder {
        StructurePiecesBuilder::default()
    }

    /// `addPiece(piece)`.
    pub fn add_piece(&mut self, piece: Box<dyn StructurePiece>) {
        self.pieces.push(piece);
    }

    /// `addPiece(piece); piece.addChildren(startPiece, builder, random)`: the Java pattern of every
    /// `generateAndAddPiece`. While the children run the piece is replaced by a stand-in with the same box (it is
    /// moved back to its index afterwards). Returns the index of the piece.
    ///
    /// `start_piece == None` means vanilla passes the piece itself (the first call on the start piece).
    pub fn add_piece_and_children(
        &mut self,
        piece: Box<dyn StructurePiece>,
        start_piece: Option<&mut dyn StructurePiece>,
        random: &mut dyn RandomSource,
    ) -> usize {
        let index = self.pieces.len();
        let stand_in = Box::new(TakenOutPiece { base: piece.base().clone() });
        self.pieces.push(stand_in);
        let mut piece = piece;
        piece.add_children(start_piece, self, random);
        self.pieces[index] = piece;
        index
    }

    /// The piece at `index`.
    pub fn get(&self, index: usize) -> &dyn StructurePiece {
        &*self.pieces[index]
    }

    pub fn get_mut(&mut self, index: usize) -> &mut dyn StructurePiece {
        &mut *self.pieces[index]
    }

    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn StructurePiece> {
        self.pieces.iter().map(|p| &**p)
    }

    /// `findCollisionPiece(box)`.
    pub fn find_collision_piece(&self, bounding_box: &BoundingBox) -> Option<&dyn StructurePiece> {
        find_collision_piece(self.iter(), bounding_box)
    }

    /// `offsetPiecesVertically(dy)`.
    pub fn offset_pieces_vertically(&mut self, dy: i32) {
        for piece in &mut self.pieces {
            piece.move_by(0, dy, 0);
        }
    }

    /// `moveBelowSeaLevel(seaLevel, minY, random, offset)`.
    pub fn move_below_sea_level(&mut self, sea_level: i32, min_y: i32, random: &mut dyn RandomSource, offset: i32) -> i32 {
        let max_y = sea_level - offset;
        let bounding_box = self.get_bounding_box();
        let mut y1_pos = bounding_box.get_y_span() + min_y + 1;
        if y1_pos < max_y {
            y1_pos += random.next_int_bounded(max_y - y1_pos);
        }
        let dy = y1_pos - bounding_box.max_y();
        self.offset_pieces_vertically(dy);
        dy
    }

    /// `moveInsideHeights(random, lowestAllowed, highestAllowed)`.
    pub fn move_inside_heights(&mut self, random: &mut dyn RandomSource, lowest_allowed: i32, highest_allowed: i32) {
        let bounding_box = self.get_bounding_box();
        let height_span = highest_allowed - lowest_allowed + 1 - bounding_box.get_y_span();
        let y0_pos = if height_span > 1 { lowest_allowed + random.next_int_bounded(height_span) } else { lowest_allowed };
        let dy = y0_pos - bounding_box.min_y();
        self.offset_pieces_vertically(dy);
    }

    /// `build()`.
    pub fn build(self) -> PiecesContainer {
        PiecesContainer::new(self.pieces)
    }

    pub fn clear(&mut self) {
        self.pieces.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// `getBoundingBox()`.
    pub fn get_bounding_box(&self) -> BoundingBox {
        create_bounding_box(self.iter())
    }
}
