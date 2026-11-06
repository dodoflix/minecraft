//! `ScatteredFeaturePiece`: the state and ground-height helpers shared by the temples, huts and igloo-like pieces. A
//! piece embeds [`ScatteredFeaturePieceData`] (its `base()` is `data.base`) and calls the helpers from `post_process`.

use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, StructurePieceSerializationContext, StructurePieceType, make_bounding_box,
};
use mc_nbt::CompoundTag;
use mc_world::heightmap::HeightmapType;

#[derive(Clone, Debug)]
pub struct ScatteredFeaturePieceData {
    pub base: PieceBase,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub height_position: i32,
}

impl ScatteredFeaturePieceData {
    /// `ScatteredFeaturePiece(type, west, floor, north, width, height, depth, direction)`.
    pub fn new(
        piece_type: &'static StructurePieceType,
        west: i32,
        floor: i32,
        north: i32,
        width: i32,
        height: i32,
        depth: i32,
        direction: Direction,
    ) -> Self {
        let mut base = PieceBase::new(
            piece_type,
            0,
            make_bounding_box(west, floor, north, direction, width, height, depth),
        );
        base.set_orientation(Some(direction));
        ScatteredFeaturePieceData {
            base,
            width,
            height,
            depth,
            height_position: -1,
        }
    }

    /// `ScatteredFeaturePiece(type, tag)`.
    pub fn load(
        piece_type: &'static StructurePieceType,
        tag: &CompoundTag,
    ) -> Result<Self, String> {
        Ok(ScatteredFeaturePieceData {
            base: PieceBase::load(piece_type, tag)?,
            width: tag.get_int_or("Width", 0),
            height: tag.get_int_or("Height", 0),
            depth: tag.get_int_or("Depth", 0),
            height_position: tag.get_int_or("HPos", 0),
        })
    }

    /// `addAdditionalSaveData(context, tag)`.
    pub fn add_additional_save_data(
        &self,
        _context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        tag.put_int("Width", self.width);
        tag.put_int("Height", self.height);
        tag.put_int("Depth", self.depth);
        tag.put_int("HPos", self.height_position);
    }

    /// `updateAverageGroundHeight(level, chunkBB, offset)`.
    pub fn update_average_ground_height(
        &mut self,
        level: &dyn LevelReader,
        chunk_bb: &BoundingBox,
        offset: i32,
    ) -> bool {
        if self.height_position >= 0 {
            return true;
        }
        let mut total = 0i32;
        let mut count = 0i32;
        let bb = self.base.bounding_box;
        for z in bb.min_z()..=bb.max_z() {
            for x in bb.min_x()..=bb.max_x() {
                if chunk_bb.is_inside(&BlockPos::new(x, 64, z)) {
                    total += level.get_height_at(HeightmapType::MotionBlockingNoLeaves, x, z);
                    count += 1;
                }
            }
        }
        if count == 0 {
            return false;
        }
        self.height_position = total / count;
        let dy = self.height_position - self.base.bounding_box.min_y() + offset;
        self.base.bounding_box.move_(0, dy, 0);
        true
    }

    /// `updateHeightPositionToLowestGroundHeight(level, offset)`.
    pub fn update_height_position_to_lowest_ground_height(
        &mut self,
        level: &dyn LevelReader,
        offset: i32,
    ) -> bool {
        if self.height_position >= 0 {
            return true;
        }
        let mut lowest_ground_height = level.get_max_y() + 1;
        let mut found_position_within_bounding_box = false;
        let bb = self.base.bounding_box;
        for z in bb.min_z()..=bb.max_z() {
            for x in bb.min_x()..=bb.max_x() {
                lowest_ground_height = lowest_ground_height.min(level.get_height_at(
                    HeightmapType::MotionBlockingNoLeaves,
                    x,
                    z,
                ));
                found_position_within_bounding_box = true;
            }
        }
        if !found_position_within_bounding_box {
            return false;
        }
        self.height_position = lowest_ground_height;
        let dy = self.height_position - self.base.bounding_box.min_y() + offset;
        self.base.bounding_box.move_(0, dy, 0);
        true
    }
}
