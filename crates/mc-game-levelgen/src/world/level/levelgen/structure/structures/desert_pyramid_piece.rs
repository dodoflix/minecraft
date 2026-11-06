//! `DesertPyramidPiece`.

use crate::world::level::levelgen::structure::scattered_feature_piece::ScatteredFeaturePieceData;
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::{Direction, Plane};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{RandomSource, create_thread_local_instance};
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_data::props;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    PieceBase, StructurePiece, StructurePieceSerializationContext, StructurePieceType,
    get_random_horizontal_direction,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;

pub const WIDTH: i32 = 21;
pub const DEPTH: i32 = 21;

/// `StructurePieceType.DESERT_PYRAMID_PIECE` (`TeDP`).
#[distributed_slice(mc_game_core::world::level::levelgen::structure::piece::STRUCTURE_PIECE_TYPES)]
pub static DESERT_PYRAMID_PIECE: StructurePieceType = StructurePieceType {
    id: "tedp",
    load: |_, tag| Ok(Box::new(DesertPyramidPiece::load(tag)?)),
};

pub struct DesertPyramidPiece {
    data: ScatteredFeaturePieceData,
    has_placed_chest: [bool; 4],
    /// Not saved: refilled by every `post_process` (the whole cellar, independent of the chunk).
    potential_suspicious_sand_world_positions: Vec<BlockPos>,
    random_collapsed_roof_pos: BlockPos,
}

impl DesertPyramidPiece {
    /// `DesertPyramidPiece(random, west, north)`.
    pub fn new(random: &mut dyn RandomSource, west: i32, north: i32) -> DesertPyramidPiece {
        let direction = get_random_horizontal_direction(random);
        DesertPyramidPiece {
            data: ScatteredFeaturePieceData::new(
                &DESERT_PYRAMID_PIECE,
                west,
                64,
                north,
                21,
                15,
                21,
                direction,
            ),
            has_placed_chest: [false; 4],
            potential_suspicious_sand_world_positions: Vec::new(),
            random_collapsed_roof_pos: BlockPos::ZERO,
        }
    }

    /// `DesertPyramidPiece(tag)`.
    pub fn load(tag: &CompoundTag) -> Result<DesertPyramidPiece, String> {
        Ok(DesertPyramidPiece {
            data: ScatteredFeaturePieceData::load(&DESERT_PYRAMID_PIECE, tag)?,
            has_placed_chest: [
                tag.get_boolean_or("hasPlacedChest0", false),
                tag.get_boolean_or("hasPlacedChest1", false),
                tag.get_boolean_or("hasPlacedChest2", false),
                tag.get_boolean_or("hasPlacedChest3", false),
            ],
            potential_suspicious_sand_world_positions: Vec::new(),
            random_collapsed_roof_pos: BlockPos::ZERO,
        })
    }

    pub fn get_potential_suspicious_sand_world_positions(&self) -> &[BlockPos] {
        &self.potential_suspicious_sand_world_positions
    }

    pub fn get_random_collapsed_roof_pos(&self) -> BlockPos {
        self.random_collapsed_roof_pos
    }

    fn add_cellar(&mut self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox) {
        let room_center = BlockPos::new(16, -4, 13);
        self.add_cellar_stairs(room_center, level, chunk_bb);
        self.add_cellar_room(room_center, level, chunk_bb);
    }

    fn add_cellar_stairs(
        &self,
        room_center: BlockPos,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
    ) {
        let (x, y, z) = (room_center.x(), room_center.y(), room_center.z());
        let sandstone_stairs = Block::SANDSTONE_STAIRS.default_state();
        let stairs = sandstone_stairs.rotate(Rotation::Counterclockwise90);
        self.place_block(level, stairs, 13, -1, 17, chunk_bb);
        self.place_block(level, stairs, 14, -2, 17, chunk_bb);
        self.place_block(level, stairs, 15, -3, 17, chunk_bb);
        let sand = Block::SAND.default_state();
        let sandstone = Block::SANDSTONE.default_state();
        let variant = level.random().next_boolean();
        self.place_block(level, sand, x - 4, y + 4, z + 4, chunk_bb);
        self.place_block(level, sand, x - 3, y + 4, z + 4, chunk_bb);
        self.place_block(level, sand, x - 2, y + 4, z + 4, chunk_bb);
        self.place_block(level, sand, x - 1, y + 4, z + 4, chunk_bb);
        self.place_block(level, sand, x, y + 4, z + 4, chunk_bb);
        self.place_block(level, sand, x - 2, y + 3, z + 4, chunk_bb);
        self.place_block(
            level,
            if variant { sand } else { sandstone },
            x - 1,
            y + 3,
            z + 4,
            chunk_bb,
        );
        self.place_block(
            level,
            if !variant { sand } else { sandstone },
            x,
            y + 3,
            z + 4,
            chunk_bb,
        );
        self.place_block(level, sand, x - 1, y + 2, z + 4, chunk_bb);
        self.place_block(level, sandstone, x, y + 2, z + 4, chunk_bb);
        self.place_block(level, sand, x, y + 1, z + 4, chunk_bb);
    }

    fn add_cellar_room(
        &mut self,
        room_center: BlockPos,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
    ) {
        let (x, y, z) = (room_center.x(), room_center.y(), room_center.z());
        let cut = Block::CUT_SANDSTONE.default_state();
        let hieroglyphs = Block::CHISELED_SANDSTONE.default_state();
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            y + 1,
            z - 3,
            x - 3,
            y + 1,
            z + 2,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x + 3,
            y + 1,
            z - 3,
            x + 3,
            y + 1,
            z + 2,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            y + 1,
            z - 3,
            x + 3,
            y + 1,
            z - 2,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            y + 1,
            z + 3,
            x + 3,
            y + 1,
            z + 3,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            y + 2,
            z - 3,
            x - 3,
            y + 2,
            z + 2,
            hieroglyphs,
            hieroglyphs,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x + 3,
            y + 2,
            z - 3,
            x + 3,
            y + 2,
            z + 2,
            hieroglyphs,
            hieroglyphs,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            y + 2,
            z - 3,
            x + 3,
            y + 2,
            z - 2,
            hieroglyphs,
            hieroglyphs,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            y + 2,
            z + 3,
            x + 3,
            y + 2,
            z + 3,
            hieroglyphs,
            hieroglyphs,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            -1,
            z - 3,
            x - 3,
            -1,
            z + 2,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x + 3,
            -1,
            z - 3,
            x + 3,
            -1,
            z + 2,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            -1,
            z - 3,
            x + 3,
            -1,
            z - 2,
            cut,
            cut,
            true,
        );
        self.generate_box(
            level,
            chunk_bb,
            x - 3,
            -1,
            z + 3,
            x + 3,
            -1,
            z + 3,
            cut,
            cut,
            true,
        );
        self.place_sand_box(x - 2, y + 1, z - 2, x + 2, y + 3, z + 2);
        self.place_collapsed_roof(level, chunk_bb, x - 2, y + 4, z - 2, x + 2, z + 2);
        let orange = Block::ORANGE_TERRACOTTA.default_state();
        let blue = Block::BLUE_TERRACOTTA.default_state();
        self.place_block(level, blue, x, y, z, chunk_bb);
        self.place_block(level, orange, x + 1, y, z - 1, chunk_bb);
        self.place_block(level, orange, x + 1, y, z + 1, chunk_bb);
        self.place_block(level, orange, x - 1, y, z - 1, chunk_bb);
        self.place_block(level, orange, x - 1, y, z + 1, chunk_bb);
        self.place_block(level, orange, x + 2, y, z, chunk_bb);
        self.place_block(level, orange, x - 2, y, z, chunk_bb);
        self.place_block(level, orange, x, y, z + 2, chunk_bb);
        self.place_block(level, orange, x, y, z - 2, chunk_bb);
        self.place_block(level, orange, x + 3, y, z, chunk_bb);
        self.place_sand(x + 3, y + 1, z);
        self.place_sand(x + 3, y + 2, z);
        self.place_block(level, cut, x + 4, y + 1, z, chunk_bb);
        self.place_block(level, hieroglyphs, x + 4, y + 2, z, chunk_bb);
        self.place_block(level, orange, x - 3, y, z, chunk_bb);
        self.place_sand(x - 3, y + 1, z);
        self.place_sand(x - 3, y + 2, z);
        self.place_block(level, cut, x - 4, y + 1, z, chunk_bb);
        self.place_block(level, hieroglyphs, x - 4, y + 2, z, chunk_bb);
        self.place_block(level, orange, x, y, z + 3, chunk_bb);
        self.place_sand(x, y + 1, z + 3);
        self.place_sand(x, y + 2, z + 3);
        self.place_block(level, orange, x, y, z - 3, chunk_bb);
        self.place_sand(x, y + 1, z - 3);
        self.place_sand(x, y + 2, z - 3);
        self.place_block(level, cut, x, y + 1, z - 4, chunk_bb);
        self.place_block(level, hieroglyphs, x, -2, z - 4, chunk_bb);
    }

    fn place_sand(&mut self, x: i32, y: i32, z: i32) {
        let world_pos = self.get_world_pos(x, y, z);
        self.potential_suspicious_sand_world_positions
            .push(world_pos);
    }

    fn place_sand_box(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    self.place_sand(x, y, z);
                }
            }
        }
    }

    fn place_collapsed_roof_piece(
        &self,
        level: &mut dyn WorldGenLevel,
        x: i32,
        y: i32,
        z: i32,
        chunk_bb: &BoundingBox,
    ) {
        let state = if level.random().next_float() < 0.33f32 {
            Block::SANDSTONE.default_state()
        } else {
            Block::SAND.default_state()
        };
        self.place_block(level, state, x, y, z, chunk_bb);
    }

    fn place_collapsed_roof(
        &mut self,
        level: &mut dyn WorldGenLevel,
        chunk_bb: &BoundingBox,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
    ) {
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.place_collapsed_roof_piece(level, x, y0, z, chunk_bb);
            }
        }
        let mut random = create_thread_local_instance(level.get_seed())
            .fork_positional()
            .at_pos(self.get_world_pos(x0, y0, z0));
        let roof_pos_x = random.next_int_between_inclusive(x0, x1);
        let roof_pos_z = random.next_int_between_inclusive(z0, z1);
        self.random_collapsed_roof_pos = BlockPos::new(
            self.get_world_x(roof_pos_x, roof_pos_z),
            self.get_world_y(y0),
            self.get_world_z(roof_pos_x, roof_pos_z),
        );
    }
}

impl StructurePiece for DesertPyramidPiece {
    fn base(&self) -> &PieceBase {
        &self.data.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.data.base
    }

    fn add_additional_save_data(
        &self,
        context: &StructurePieceSerializationContext<'_>,
        tag: &mut CompoundTag,
    ) {
        self.data.add_additional_save_data(context, tag);
        for (i, placed) in self.has_placed_chest.iter().enumerate() {
            tag.put_boolean(format!("hasPlacedChest{i}"), *placed);
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
        let offset = -random.next_int_bounded(3);
        if !self
            .data
            .update_height_position_to_lowest_ground_height(level, offset)
        {
            return;
        }
        let (width, depth) = (self.data.width, self.data.depth);
        let sandstone = Block::SANDSTONE.default_state();
        let air = Block::AIR.default_state();
        let cut = Block::CUT_SANDSTONE.default_state();
        let chiseled = Block::CHISELED_SANDSTONE.default_state();
        let orange = Block::ORANGE_TERRACOTTA.default_state();
        let blue = Block::BLUE_TERRACOTTA.default_state();
        let bb = chunk_bb;
        macro_rules! boxed {
            ($x0:expr, $y0:expr, $z0:expr, $x1:expr, $y1:expr, $z1:expr, $edge:expr, $fill:expr) => {
                self.generate_box(level, bb, $x0, $y0, $z0, $x1, $y1, $z1, $edge, $fill, false)
            };
        }
        macro_rules! put {
            ($state:expr, $x:expr, $y:expr, $z:expr) => {
                self.place_block(level, $state, $x, $y, $z, bb)
            };
        }

        boxed!(0, -4, 0, width - 1, 0, depth - 1, sandstone, sandstone);
        for pos in 1..=9 {
            boxed!(
                pos,
                pos,
                pos,
                width - 1 - pos,
                pos,
                depth - 1 - pos,
                sandstone,
                sandstone
            );
            boxed!(
                pos + 1,
                pos,
                pos + 1,
                width - 2 - pos,
                pos,
                depth - 2 - pos,
                air,
                air
            );
        }
        for x in 0..width {
            for z in 0..depth {
                self.fill_column_down(level, sandstone, x, -5, z, bb);
            }
        }

        let north_stairs = Block::SANDSTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::North);
        let south_stairs = Block::SANDSTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::South);
        let east_stairs = Block::SANDSTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::East);
        let west_stairs = Block::SANDSTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::West);
        boxed!(0, 0, 0, 4, 9, 4, sandstone, air);
        boxed!(1, 10, 1, 3, 10, 3, sandstone, sandstone);
        put!(north_stairs, 2, 10, 0);
        put!(south_stairs, 2, 10, 4);
        put!(east_stairs, 0, 10, 2);
        put!(west_stairs, 4, 10, 2);
        boxed!(width - 5, 0, 0, width - 1, 9, 4, sandstone, air);
        boxed!(width - 4, 10, 1, width - 2, 10, 3, sandstone, sandstone);
        put!(north_stairs, width - 3, 10, 0);
        put!(south_stairs, width - 3, 10, 4);
        put!(east_stairs, width - 5, 10, 2);
        put!(west_stairs, width - 1, 10, 2);
        boxed!(8, 0, 0, 12, 4, 4, sandstone, air);
        boxed!(9, 1, 0, 11, 3, 4, air, air);
        put!(cut, 9, 1, 1);
        put!(cut, 9, 2, 1);
        put!(cut, 9, 3, 1);
        put!(cut, 10, 3, 1);
        put!(cut, 11, 3, 1);
        put!(cut, 11, 2, 1);
        put!(cut, 11, 1, 1);
        boxed!(4, 1, 1, 8, 3, 3, sandstone, air);
        boxed!(4, 1, 2, 8, 2, 2, air, air);
        boxed!(12, 1, 1, 16, 3, 3, sandstone, air);
        boxed!(12, 1, 2, 16, 2, 2, air, air);
        boxed!(5, 4, 5, width - 6, 4, depth - 6, sandstone, sandstone);
        boxed!(9, 4, 9, 11, 4, 11, air, air);
        boxed!(8, 1, 8, 8, 3, 8, cut, cut);
        boxed!(12, 1, 8, 12, 3, 8, cut, cut);
        boxed!(8, 1, 12, 8, 3, 12, cut, cut);
        boxed!(12, 1, 12, 12, 3, 12, cut, cut);
        boxed!(1, 1, 5, 4, 4, 11, sandstone, sandstone);
        boxed!(width - 5, 1, 5, width - 2, 4, 11, sandstone, sandstone);
        boxed!(6, 7, 9, 6, 7, 11, sandstone, sandstone);
        boxed!(width - 7, 7, 9, width - 7, 7, 11, sandstone, sandstone);
        boxed!(5, 5, 9, 5, 7, 11, cut, cut);
        boxed!(width - 6, 5, 9, width - 6, 7, 11, cut, cut);
        put!(air, 5, 5, 10);
        put!(air, 5, 6, 10);
        put!(air, 6, 6, 10);
        put!(air, width - 6, 5, 10);
        put!(air, width - 6, 6, 10);
        put!(air, width - 7, 6, 10);
        boxed!(2, 4, 4, 2, 6, 4, air, air);
        boxed!(width - 3, 4, 4, width - 3, 6, 4, air, air);
        put!(north_stairs, 2, 4, 5);
        put!(north_stairs, 2, 3, 4);
        put!(north_stairs, width - 3, 4, 5);
        put!(north_stairs, width - 3, 3, 4);
        boxed!(1, 1, 3, 2, 2, 3, sandstone, sandstone);
        boxed!(width - 3, 1, 3, width - 2, 2, 3, sandstone, sandstone);
        put!(sandstone, 1, 1, 2);
        put!(sandstone, width - 2, 1, 2);
        put!(Block::SANDSTONE_SLAB.default_state(), 1, 2, 2);
        put!(Block::SANDSTONE_SLAB.default_state(), width - 2, 2, 2);
        put!(west_stairs, 2, 1, 2);
        put!(east_stairs, width - 3, 1, 2);
        boxed!(4, 3, 5, 4, 3, 17, sandstone, sandstone);
        boxed!(width - 5, 3, 5, width - 5, 3, 17, sandstone, sandstone);
        boxed!(3, 1, 5, 4, 2, 16, air, air);
        boxed!(width - 6, 1, 5, width - 5, 2, 16, air, air);

        let mut z = 5;
        while z <= 17 {
            put!(cut, 4, 1, z);
            put!(chiseled, 4, 2, z);
            put!(cut, width - 5, 1, z);
            put!(chiseled, width - 5, 2, z);
            z += 2;
        }

        put!(orange, 10, 0, 7);
        put!(orange, 10, 0, 8);
        put!(orange, 9, 0, 9);
        put!(orange, 11, 0, 9);
        put!(orange, 8, 0, 10);
        put!(orange, 12, 0, 10);
        put!(orange, 7, 0, 10);
        put!(orange, 13, 0, 10);
        put!(orange, 9, 0, 11);
        put!(orange, 11, 0, 11);
        put!(orange, 10, 0, 12);
        put!(orange, 10, 0, 13);
        put!(blue, 10, 0, 10);

        let mut x = 0;
        while x < width {
            put!(cut, x, 2, 1);
            put!(orange, x, 2, 2);
            put!(cut, x, 2, 3);
            put!(cut, x, 3, 1);
            put!(orange, x, 3, 2);
            put!(cut, x, 3, 3);
            put!(orange, x, 4, 1);
            put!(chiseled, x, 4, 2);
            put!(orange, x, 4, 3);
            put!(cut, x, 5, 1);
            put!(orange, x, 5, 2);
            put!(cut, x, 5, 3);
            put!(orange, x, 6, 1);
            put!(chiseled, x, 6, 2);
            put!(orange, x, 6, 3);
            put!(orange, x, 7, 1);
            put!(orange, x, 7, 2);
            put!(orange, x, 7, 3);
            put!(cut, x, 8, 1);
            put!(cut, x, 8, 2);
            put!(cut, x, 8, 3);
            x += width - 1;
        }

        let mut x = 2;
        while x <= width - 3 {
            put!(cut, x - 1, 2, 0);
            put!(orange, x, 2, 0);
            put!(cut, x + 1, 2, 0);
            put!(cut, x - 1, 3, 0);
            put!(orange, x, 3, 0);
            put!(cut, x + 1, 3, 0);
            put!(orange, x - 1, 4, 0);
            put!(chiseled, x, 4, 0);
            put!(orange, x + 1, 4, 0);
            put!(cut, x - 1, 5, 0);
            put!(orange, x, 5, 0);
            put!(cut, x + 1, 5, 0);
            put!(orange, x - 1, 6, 0);
            put!(chiseled, x, 6, 0);
            put!(orange, x + 1, 6, 0);
            put!(orange, x - 1, 7, 0);
            put!(orange, x, 7, 0);
            put!(orange, x + 1, 7, 0);
            put!(cut, x - 1, 8, 0);
            put!(cut, x, 8, 0);
            put!(cut, x + 1, 8, 0);
            x += width - 3 - 2;
        }

        boxed!(8, 4, 0, 12, 6, 0, cut, cut);
        put!(air, 8, 6, 0);
        put!(air, 12, 6, 0);
        put!(orange, 9, 5, 0);
        put!(chiseled, 10, 5, 0);
        put!(orange, 11, 5, 0);
        boxed!(8, -14, 8, 12, -11, 12, cut, cut);
        boxed!(8, -10, 8, 12, -10, 12, chiseled, chiseled);
        boxed!(8, -9, 8, 12, -9, 12, cut, cut);
        boxed!(8, -8, 8, 12, -1, 12, sandstone, sandstone);
        boxed!(9, -11, 9, 11, -1, 11, air, air);
        put!(Block::STONE_PRESSURE_PLATE.default_state(), 10, -11, 10);
        boxed!(9, -13, 9, 11, -13, 11, Block::TNT.default_state(), air);
        put!(air, 8, -11, 10);
        put!(air, 8, -10, 10);
        put!(chiseled, 7, -10, 10);
        put!(cut, 7, -11, 10);
        put!(air, 12, -11, 10);
        put!(air, 12, -10, 10);
        put!(chiseled, 13, -10, 10);
        put!(cut, 13, -11, 10);
        put!(air, 10, -11, 8);
        put!(air, 10, -10, 8);
        put!(chiseled, 10, -10, 7);
        put!(cut, 10, -11, 7);
        put!(air, 10, -11, 12);
        put!(air, 10, -10, 12);
        put!(chiseled, 10, -10, 13);
        put!(cut, 10, -11, 13);

        for direction in Plane::Horizontal.iter() {
            let index = direction.get_2d_data_value() as usize;
            if !self.has_placed_chest[index] {
                let xo = direction.get_step_x() * 2;
                let zo = direction.get_step_z() * 2;
                self.has_placed_chest[index] = self.create_chest(
                    level,
                    bb,
                    random,
                    10 + xo,
                    -11,
                    10 + zo,
                    "minecraft:chests/desert_pyramid",
                );
            }
        }

        self.add_cellar(level, bb);
    }
}
