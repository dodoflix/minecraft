//! `JungleTemplePiece`.

use crate::world::level::levelgen::structure::scattered_feature_piece::ScatteredFeaturePieceData;
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{AttachFace, RedstoneSide};
use mc_data::props;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::piece::{
    BlockSelector, PieceBase, STRUCTURE_PIECE_TYPES, StructurePiece,
    StructurePieceSerializationContext, StructurePieceType, get_random_horizontal_direction,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::CompoundTag;

pub const WIDTH: i32 = 12;
pub const DEPTH: i32 = 15;

/// `StructurePieceType.JUNGLE_PYRAMID_PIECE` (`TeJP`).
#[distributed_slice(STRUCTURE_PIECE_TYPES)]
pub static JUNGLE_PYRAMID_PIECE: StructurePieceType = StructurePieceType {
    id: "tejp",
    load: |_, tag| Ok(Box::new(JungleTemplePiece::load(tag)?)),
};

pub struct JungleTemplePiece {
    data: ScatteredFeaturePieceData,
    placed_main_chest: bool,
    placed_hidden_chest: bool,
    placed_trap_1: bool,
    placed_trap_2: bool,
}

/// `JungleTemplePiece.MossStoneSelector`.
struct MossStoneSelector {
    next: BlockState,
}

impl BlockSelector for MossStoneSelector {
    fn next(
        &mut self,
        random: &mut dyn RandomSource,
        _world_x: i32,
        _world_y: i32,
        _world_z: i32,
        _is_edge: bool,
    ) {
        self.next = if random.next_float() < 0.4f32 {
            Block::COBBLESTONE.default_state()
        } else {
            Block::MOSSY_COBBLESTONE.default_state()
        };
    }

    fn get_next(&self) -> BlockState {
        self.next
    }
}

impl JungleTemplePiece {
    /// `JungleTemplePiece(random, west, north)`.
    pub fn new(random: &mut dyn RandomSource, west: i32, north: i32) -> JungleTemplePiece {
        let direction = get_random_horizontal_direction(random);
        JungleTemplePiece {
            data: ScatteredFeaturePieceData::new(
                &JUNGLE_PYRAMID_PIECE,
                west,
                64,
                north,
                12,
                10,
                15,
                direction,
            ),
            placed_main_chest: false,
            placed_hidden_chest: false,
            placed_trap_1: false,
            placed_trap_2: false,
        }
    }

    /// `JungleTemplePiece(tag)`.
    pub fn load(tag: &CompoundTag) -> Result<JungleTemplePiece, String> {
        Ok(JungleTemplePiece {
            data: ScatteredFeaturePieceData::load(&JUNGLE_PYRAMID_PIECE, tag)?,
            placed_main_chest: tag.get_boolean_or("placedMainChest", false),
            placed_hidden_chest: tag.get_boolean_or("placedHiddenChest", false),
            placed_trap_1: tag.get_boolean_or("placedTrap1", false),
            placed_trap_2: tag.get_boolean_or("placedTrap2", false),
        })
    }
}

impl StructurePiece for JungleTemplePiece {
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
        tag.put_boolean("placedMainChest", self.placed_main_chest);
        tag.put_boolean("placedHiddenChest", self.placed_hidden_chest);
        tag.put_boolean("placedTrap1", self.placed_trap_1);
        tag.put_boolean("placedTrap2", self.placed_trap_2);
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
        if !self.data.update_average_ground_height(level, chunk_bb, 0) {
            return;
        }
        let (width, depth) = (self.data.width, self.data.depth);
        let bb = chunk_bb;
        let mut selector = MossStoneSelector {
            next: Block::AIR.default_state(),
        };
        macro_rules! sel {
            ($x0:expr, $y0:expr, $z0:expr, $x1:expr, $y1:expr, $z1:expr) => {
                self.generate_box_selector(
                    level,
                    bb,
                    $x0,
                    $y0,
                    $z0,
                    $x1,
                    $y1,
                    $z1,
                    false,
                    random,
                    &mut selector,
                )
            };
        }
        macro_rules! air_box {
            ($x0:expr, $y0:expr, $z0:expr, $x1:expr, $y1:expr, $z1:expr) => {
                self.generate_air_box(level, bb, $x0, $y0, $z0, $x1, $y1, $z1)
            };
        }
        macro_rules! put {
            ($state:expr, $x:expr, $y:expr, $z:expr) => {
                self.place_block(level, $state, $x, $y, $z, bb)
            };
        }
        sel!(0, -4, 0, width - 1, 0, depth - 1);
        sel!(2, 1, 2, 9, 2, 2);
        sel!(2, 1, 12, 9, 2, 12);
        sel!(2, 1, 3, 2, 2, 11);
        sel!(9, 1, 3, 9, 2, 11);
        sel!(1, 3, 1, 10, 6, 1);
        sel!(1, 3, 13, 10, 6, 13);
        sel!(1, 3, 2, 1, 6, 12);
        sel!(10, 3, 2, 10, 6, 12);
        sel!(2, 3, 2, 9, 3, 12);
        sel!(2, 6, 2, 9, 6, 12);
        sel!(3, 7, 3, 8, 7, 11);
        sel!(4, 8, 4, 7, 8, 10);
        air_box!(3, 1, 3, 8, 2, 11);
        air_box!(4, 3, 6, 7, 3, 9);
        air_box!(2, 4, 2, 9, 5, 12);
        air_box!(4, 6, 5, 7, 6, 9);
        air_box!(5, 7, 6, 6, 7, 8);
        air_box!(5, 1, 2, 6, 2, 2);
        air_box!(5, 2, 12, 6, 2, 12);
        air_box!(5, 5, 1, 6, 5, 1);
        air_box!(5, 5, 13, 6, 5, 13);
        put!(Block::AIR.default_state(), 1, 5, 5);
        put!(Block::AIR.default_state(), 10, 5, 5);
        put!(Block::AIR.default_state(), 1, 5, 9);
        put!(Block::AIR.default_state(), 10, 5, 9);
        for z in (0..=14).step_by(14) {
            sel!(2, 4, z, 2, 5, z);
            sel!(4, 4, z, 4, 5, z);
            sel!(7, 4, z, 7, 5, z);
            sel!(9, 4, z, 9, 5, z);
        }
        sel!(5, 6, 0, 6, 6, 0);
        for x in (0..=11).step_by(11) {
            for z in (2..=12).step_by(2) {
                sel!(x, 4, z, x, 5, z);
            }
            sel!(x, 6, 5, x, 6, 5);
            sel!(x, 6, 9, x, 6, 9);
        }
        sel!(2, 7, 2, 2, 9, 2);
        sel!(9, 7, 2, 9, 9, 2);
        sel!(2, 7, 12, 2, 9, 12);
        sel!(9, 7, 12, 9, 9, 12);
        sel!(4, 9, 4, 4, 9, 4);
        sel!(7, 9, 4, 7, 9, 4);
        sel!(4, 9, 10, 4, 9, 10);
        sel!(7, 9, 10, 7, 9, 10);
        sel!(5, 9, 7, 6, 9, 7);
        let east_stairs = Block::COBBLESTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::East);
        let west_stairs = Block::COBBLESTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::West);
        let south_stairs = Block::COBBLESTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::South);
        let north_stairs = Block::COBBLESTONE_STAIRS
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::North);
        put!(north_stairs, 5, 9, 6);
        put!(north_stairs, 6, 9, 6);
        put!(south_stairs, 5, 9, 8);
        put!(south_stairs, 6, 9, 8);
        put!(north_stairs, 4, 0, 0);
        put!(north_stairs, 5, 0, 0);
        put!(north_stairs, 6, 0, 0);
        put!(north_stairs, 7, 0, 0);
        put!(north_stairs, 4, 1, 8);
        put!(north_stairs, 4, 2, 9);
        put!(north_stairs, 4, 3, 10);
        put!(north_stairs, 7, 1, 8);
        put!(north_stairs, 7, 2, 9);
        put!(north_stairs, 7, 3, 10);
        sel!(4, 1, 9, 4, 1, 9);
        sel!(7, 1, 9, 7, 1, 9);
        sel!(4, 1, 10, 7, 2, 10);
        sel!(5, 4, 5, 6, 4, 5);
        put!(east_stairs, 4, 4, 5);
        put!(west_stairs, 7, 4, 5);
        for i in 0..4 {
            put!(south_stairs, 5, 0 - i, 6 + i);
            put!(south_stairs, 6, 0 - i, 6 + i);
            air_box!(5, 0 - i, 7 + i, 6, 0 - i, 9 + i);
        }
        air_box!(1, -3, 12, 10, -1, 13);
        air_box!(1, -3, 1, 3, -1, 13);
        air_box!(1, -3, 1, 9, -1, 5);
        for z in (1..=13).step_by(2) {
            sel!(1, -3, z, 1, -2, z);
        }
        for z in (2..=12).step_by(2) {
            sel!(1, -1, z, 3, -1, z);
        }
        sel!(2, -2, 1, 5, -2, 1);
        sel!(7, -2, 1, 9, -2, 1);
        sel!(6, -3, 1, 6, -3, 1);
        sel!(6, -1, 1, 6, -1, 1);
        put!(
            Block::TRIPWIRE_HOOK
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::East)
                .set(props::ATTACHED, true),
            1,
            -3,
            8
        );
        put!(
            Block::TRIPWIRE_HOOK
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::West)
                .set(props::ATTACHED, true),
            4,
            -3,
            8
        );
        put!(
            Block::TRIPWIRE
                .default_state()
                .set(props::EAST, true)
                .set(props::WEST, true)
                .set(props::ATTACHED, true),
            2,
            -3,
            8
        );
        put!(
            Block::TRIPWIRE
                .default_state()
                .set(props::EAST, true)
                .set(props::WEST, true)
                .set(props::ATTACHED, true),
            3,
            -3,
            8
        );
        let redstone_wire_ns = Block::REDSTONE_WIRE
            .default_state()
            .set(props::NORTH_REDSTONE, RedstoneSide::Side)
            .set(props::SOUTH_REDSTONE, RedstoneSide::Side);
        put!(redstone_wire_ns, 5, -3, 7);
        put!(redstone_wire_ns, 5, -3, 6);
        put!(redstone_wire_ns, 5, -3, 5);
        put!(redstone_wire_ns, 5, -3, 4);
        put!(redstone_wire_ns, 5, -3, 3);
        put!(redstone_wire_ns, 5, -3, 2);
        put!(
            Block::REDSTONE_WIRE
                .default_state()
                .set(props::NORTH_REDSTONE, RedstoneSide::Side)
                .set(props::WEST_REDSTONE, RedstoneSide::Side),
            5,
            -3,
            1
        );
        put!(
            Block::REDSTONE_WIRE
                .default_state()
                .set(props::EAST_REDSTONE, RedstoneSide::Side)
                .set(props::WEST_REDSTONE, RedstoneSide::Side),
            4,
            -3,
            1
        );
        put!(Block::MOSSY_COBBLESTONE.default_state(), 3, -3, 1);
        if !self.placed_trap_1 {
            self.placed_trap_1 = self.create_dispenser(
                level,
                bb,
                random,
                3,
                -2,
                1,
                Direction::North,
                "minecraft:chests/jungle_temple_dispenser",
            );
        }
        put!(
            Block::VINE.default_state().set(props::SOUTH, true),
            3,
            -2,
            2
        );
        put!(
            Block::TRIPWIRE_HOOK
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::North)
                .set(props::ATTACHED, true),
            7,
            -3,
            1
        );
        put!(
            Block::TRIPWIRE_HOOK
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::South)
                .set(props::ATTACHED, true),
            7,
            -3,
            5
        );
        put!(
            Block::TRIPWIRE
                .default_state()
                .set(props::NORTH, true)
                .set(props::SOUTH, true)
                .set(props::ATTACHED, true),
            7,
            -3,
            2
        );
        put!(
            Block::TRIPWIRE
                .default_state()
                .set(props::NORTH, true)
                .set(props::SOUTH, true)
                .set(props::ATTACHED, true),
            7,
            -3,
            3
        );
        put!(
            Block::TRIPWIRE
                .default_state()
                .set(props::NORTH, true)
                .set(props::SOUTH, true)
                .set(props::ATTACHED, true),
            7,
            -3,
            4
        );
        put!(
            Block::REDSTONE_WIRE
                .default_state()
                .set(props::EAST_REDSTONE, RedstoneSide::Side)
                .set(props::WEST_REDSTONE, RedstoneSide::Side),
            8,
            -3,
            6
        );
        put!(
            Block::REDSTONE_WIRE
                .default_state()
                .set(props::WEST_REDSTONE, RedstoneSide::Side)
                .set(props::SOUTH_REDSTONE, RedstoneSide::Side),
            9,
            -3,
            6
        );
        put!(
            Block::REDSTONE_WIRE
                .default_state()
                .set(props::NORTH_REDSTONE, RedstoneSide::Side)
                .set(props::SOUTH_REDSTONE, RedstoneSide::Up),
            9,
            -3,
            5
        );
        put!(Block::MOSSY_COBBLESTONE.default_state(), 9, -3, 4);
        put!(redstone_wire_ns, 9, -2, 4);
        if !self.placed_trap_2 {
            self.placed_trap_2 = self.create_dispenser(
                level,
                bb,
                random,
                9,
                -2,
                3,
                Direction::West,
                "minecraft:chests/jungle_temple_dispenser",
            );
        }
        put!(Block::VINE.default_state().set(props::EAST, true), 8, -1, 3);
        put!(Block::VINE.default_state().set(props::EAST, true), 8, -2, 3);
        if !self.placed_main_chest {
            self.placed_main_chest = self.create_chest(
                level,
                bb,
                random,
                8,
                -3,
                3,
                "minecraft:chests/jungle_temple",
            );
        }
        put!(Block::MOSSY_COBBLESTONE.default_state(), 9, -3, 2);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 8, -3, 1);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 4, -3, 5);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 5, -2, 5);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 5, -1, 5);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 6, -3, 5);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 7, -2, 5);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 7, -1, 5);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 8, -3, 5);
        sel!(9, -1, 1, 9, -1, 5);
        air_box!(8, -3, 8, 10, -1, 10);
        put!(Block::CHISELED_STONE_BRICKS.default_state(), 8, -2, 11);
        put!(Block::CHISELED_STONE_BRICKS.default_state(), 9, -2, 11);
        put!(Block::CHISELED_STONE_BRICKS.default_state(), 10, -2, 11);
        let lever = Block::LEVER
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::North)
            .set(props::ATTACH_FACE, AttachFace::Wall);
        put!(lever, 8, -2, 12);
        put!(lever, 9, -2, 12);
        put!(lever, 10, -2, 12);
        sel!(8, -3, 8, 8, -3, 10);
        sel!(10, -3, 8, 10, -3, 10);
        put!(Block::MOSSY_COBBLESTONE.default_state(), 10, -2, 9);
        put!(redstone_wire_ns, 8, -2, 9);
        put!(redstone_wire_ns, 8, -2, 10);
        put!(
            Block::REDSTONE_WIRE
                .default_state()
                .set(props::NORTH_REDSTONE, RedstoneSide::Side)
                .set(props::SOUTH_REDSTONE, RedstoneSide::Side)
                .set(props::EAST_REDSTONE, RedstoneSide::Side)
                .set(props::WEST_REDSTONE, RedstoneSide::Side),
            10,
            -1,
            9
        );
        put!(
            Block::STICKY_PISTON
                .default_state()
                .set(props::FACING, Direction::Up),
            9,
            -2,
            8
        );
        put!(
            Block::STICKY_PISTON
                .default_state()
                .set(props::FACING, Direction::West),
            10,
            -2,
            8
        );
        put!(
            Block::STICKY_PISTON
                .default_state()
                .set(props::FACING, Direction::West),
            10,
            -1,
            8
        );
        put!(
            Block::REPEATER
                .default_state()
                .set(props::HORIZONTAL_FACING, Direction::North),
            10,
            -2,
            10
        );
        if !self.placed_hidden_chest {
            self.placed_hidden_chest = self.create_chest(
                level,
                bb,
                random,
                9,
                -3,
                10,
                "minecraft:chests/jungle_temple",
            );
        }
    }
}
