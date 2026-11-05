//! `SwampHutPiece`.

use super::spawned_mob_tag::{cat_entity_tag, witch_entity_tag};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::piece::{
    PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece, StructurePieceSerializationContext, StructurePieceType, get_random_horizontal_direction,
};
use crate::world::level::levelgen::structure::scattered_feature_piece::ScatteredFeaturePieceData;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::StairsShape;
use mc_data::props;
use mc_nbt::CompoundTag;

/// `StructurePieceType.SWAMPLAND_HUT` (`TeSH`).
#[distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static SWAMPLAND_HUT: StructurePieceType = StructurePieceType { id: "tesh", load: |_, tag| Ok(Box::new(SwampHutPiece::load(tag)?)) };

pub struct SwampHutPiece {
    data: ScatteredFeaturePieceData,
    spawned_witch: bool,
    spawned_cat: bool,
}

impl SwampHutPiece {
    /// `SwampHutPiece(random, west, north)`.
    pub fn new(random: &mut dyn RandomSource, west: i32, north: i32) -> SwampHutPiece {
        let direction = get_random_horizontal_direction(random);
        SwampHutPiece { data: ScatteredFeaturePieceData::new(&SWAMPLAND_HUT, west, 64, north, 7, 7, 9, direction), spawned_witch: false, spawned_cat: false }
    }

    /// `SwampHutPiece(tag)`.
    pub fn load(tag: &CompoundTag) -> Result<SwampHutPiece, String> {
        Ok(SwampHutPiece {
            data: ScatteredFeaturePieceData::load(&SWAMPLAND_HUT, tag)?,
            spawned_witch: tag.get_boolean_or("Witch", false),
            spawned_cat: tag.get_boolean_or("Cat", false),
        })
    }

    /// `spawnCat(level, chunkBB)`.
    fn spawn_cat(&mut self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox) {
        if !self.spawned_cat {
            let pos = self.get_world_pos(2, 2, 5);
            if chunk_bb.is_inside(&pos) {
                self.spawned_cat = true;
                level.add_fresh_entity_nbt(cat_entity_tag(pos));
            }
        }
    }
}

impl StructurePiece for SwampHutPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(&self, context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        self.data.add_additional_save_data(context, tag);
        tag.put_boolean("Witch", self.spawned_witch);
        tag.put_boolean("Cat", self.spawned_cat);
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
        if !self.data.update_average_ground_height(level, chunk_bb, 0) {
            return;
        }
        let planks = Block::SPRUCE_PLANKS.default_state();
        let log = Block::OAK_LOG.default_state();
        let bb = chunk_bb;
        macro_rules! boxed {
            ($x0:expr, $y0:expr, $z0:expr, $x1:expr, $y1:expr, $z1:expr, $state:expr) => {
                self.generate_box(level, bb, $x0, $y0, $z0, $x1, $y1, $z1, $state, $state, false)
            };
        }
        macro_rules! put {
            ($state:expr, $x:expr, $y:expr, $z:expr) => {
                self.place_block(level, $state, $x, $y, $z, bb)
            };
        }
        boxed!(1, 1, 1, 5, 1, 7, planks);
        boxed!(1, 4, 2, 5, 4, 7, planks);
        boxed!(2, 1, 0, 4, 1, 0, planks);
        boxed!(2, 2, 2, 3, 3, 2, planks);
        boxed!(1, 2, 3, 1, 3, 6, planks);
        boxed!(5, 2, 3, 5, 3, 6, planks);
        boxed!(2, 2, 7, 4, 3, 7, planks);
        boxed!(1, 0, 2, 1, 3, 2, log);
        boxed!(5, 0, 2, 5, 3, 2, log);
        boxed!(1, 0, 7, 1, 3, 7, log);
        boxed!(5, 0, 7, 5, 3, 7, log);
        put!(Block::OAK_FENCE.default_state(), 2, 3, 2);
        put!(Block::OAK_FENCE.default_state(), 3, 3, 7);
        put!(Block::AIR.default_state(), 1, 3, 4);
        put!(Block::AIR.default_state(), 5, 3, 4);
        put!(Block::AIR.default_state(), 5, 3, 5);
        put!(Block::POTTED_RED_MUSHROOM.default_state(), 1, 3, 5);
        put!(Block::CRAFTING_TABLE.default_state(), 3, 2, 6);
        put!(Block::CAULDRON.default_state(), 4, 2, 6);
        put!(Block::OAK_FENCE.default_state(), 1, 2, 1);
        put!(Block::OAK_FENCE.default_state(), 5, 2, 1);
        let stairs = Block::SPRUCE_STAIRS.default_state();
        let north_stairs = stairs.set(props::HORIZONTAL_FACING, Direction::North);
        let east_stairs = stairs.set(props::HORIZONTAL_FACING, Direction::East);
        let west_stairs = stairs.set(props::HORIZONTAL_FACING, Direction::West);
        let south_stairs = stairs.set(props::HORIZONTAL_FACING, Direction::South);
        boxed!(0, 4, 1, 6, 4, 1, north_stairs);
        boxed!(0, 4, 2, 0, 4, 7, east_stairs);
        boxed!(6, 4, 2, 6, 4, 7, west_stairs);
        boxed!(0, 4, 8, 6, 4, 8, south_stairs);
        put!(north_stairs.set(props::STAIRS_SHAPE, StairsShape::OuterRight), 0, 4, 1);
        put!(north_stairs.set(props::STAIRS_SHAPE, StairsShape::OuterLeft), 6, 4, 1);
        put!(south_stairs.set(props::STAIRS_SHAPE, StairsShape::OuterLeft), 0, 4, 8);
        put!(south_stairs.set(props::STAIRS_SHAPE, StairsShape::OuterRight), 6, 4, 8);

        let mut z = 2;
        while z <= 7 {
            let mut x = 1;
            while x <= 5 {
                self.fill_column_down(level, log, x, -1, z, bb);
                x += 4;
            }
            z += 5;
        }

        if !self.spawned_witch {
            let pos = self.get_world_pos(2, 2, 5);
            if chunk_bb.is_inside(&pos) {
                self.spawned_witch = true;
                level.add_fresh_entity_nbt(witch_entity_tag(pos));
            }
        }
        self.spawn_cat(level, chunk_bb);
    }
}
