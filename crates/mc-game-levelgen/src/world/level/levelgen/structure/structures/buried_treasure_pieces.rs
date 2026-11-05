//! `BuriedTreasurePieces`.

use crate::world::level::BlockGetter as _;
use crate::world::level::LevelAccessor as _;
use crate::world::level::LevelReader as _;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::piece::{PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext, StructurePieceType};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_nbt::CompoundTag;
use mc_world::heightmap::HeightmapType;

/// `StructurePieceType.BURIED_TREASURE_PIECE` (`BTP`).
#[distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static BURIED_TREASURE_PIECE: StructurePieceType = StructurePieceType {
    id: "btp",
    load: |_, tag| Ok(Box::new(BuriedTreasurePiece { base: PieceBase::load(&BURIED_TREASURE_PIECE, tag)? })),
};

/// `BuriedTreasurePieces.BuriedTreasurePiece`.
pub struct BuriedTreasurePiece {
    base: PieceBase,
}

impl BuriedTreasurePiece {
    /// `BuriedTreasurePiece(offset)`.
    pub fn new(offset: BlockPos) -> BuriedTreasurePiece {
        BuriedTreasurePiece { base: PieceBase::new(&BURIED_TREASURE_PIECE, 0, BoundingBox::from_block_pos(&offset)) }
    }
}

fn is_liquid(block_state: BlockState) -> bool {
    matches!(block_state.block(), Block::WATER | Block::LAVA)
}

impl StructurePiece for BuriedTreasurePiece {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn add_additional_save_data(&self, _context: &StructurePieceSerializationContext<'_>, _tag: &mut CompoundTag) {}

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
        let y = level.get_height_at(HeightmapType::OceanFloorWg, self.base.bounding_box.min_x(), self.base.bounding_box.min_z());
        let mut pos = BlockPos::new(self.base.bounding_box.min_x(), y, self.base.bounding_box.min_z());
        while pos.y() > level.get_min_y() {
            let current_state = level.get_block_state(pos);
            let below_state = level.get_block_state(pos.below());
            if matches!(below_state.block(), Block::SANDSTONE | Block::STONE | Block::ANDESITE | Block::GRANITE | Block::DIORITE) {
                let soft_state = if !current_state.is_air() && !is_liquid(current_state) { current_state } else { Block::SAND.default_state() };
                for direction in Direction::ALL {
                    let relative_pos = pos.relative(direction);
                    let relative_state = level.get_block_state(relative_pos);
                    if relative_state.is_air() || is_liquid(relative_state) {
                        let below_relative_pos = relative_pos.below();
                        let below_relative_state = level.get_block_state(below_relative_pos);
                        if (below_relative_state.is_air() || is_liquid(below_relative_state)) && direction != Direction::Up {
                            level.set_block_and_update(relative_pos, below_state);
                        } else {
                            level.set_block_and_update(relative_pos, soft_state);
                        }
                    }
                }
                self.base.bounding_box = BoundingBox::from_block_pos(&pos);
                self.create_chest_at(level, chunk_bb, random, pos, "minecraft:chests/buried_treasure", None);
                return;
            }
            pos = pos.offset(0, -1, 0);
        }
    }
}
