//! `MineshaftPieces`: the pieces of an abandoned mineshaft (`mscorridor`, `mscrossing`, `msroom`, `msstairs`) and the
//! recursive generation (`createRandomShaftPiece`/`generateAndAddPiece`) that grows them from the start room.
//!
//! `MineShaftPiece` (the abstract base class) is the [`MineShaftPiece`] trait: the piece-type specific block set
//! ([`MineshaftType`]) and the helpers shared by the four pieces. The start piece vanilla hands down through
//! `addChildren` is only read for its bounding box (and the `type`, which is the same in every piece of one start), so
//! the stand-in the builder holds for a piece under construction is enough for everything but the room, which is always
//! the root and is handed down as `self`.

use super::mineshaft_structure::MineshaftType;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers::can_support_center;
use crate::world::level::levelgen::block_entity_data::spawner_tag;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::piece::{
    PieceBase, StructurePiece, StructurePieceSerializationContext, StructurePieceType, bounding_box_to_ints, cave_air, read_bounding_box,
};
use crate::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::{Axis, Direction};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::block::{Block, BlockState};
use mc_data::enums::RailShape;
use mc_data::props;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::sync::LazyLock;

/// `BuiltInLootTables.ABANDONED_MINESHAFT`.
const ABANDONED_MINESHAFT: &str = "minecraft:chests/abandoned_mineshaft";

/// `BiomeTags.MINESHAFT_BLOCKING`.
static MINESHAFT_BLOCKING: LazyLock<Vec<Identifier>> = LazyLock::new(|| {
    mc_data::tags::names("worldgen/biome", "minecraft:mineshaft_blocking")
        .unwrap_or_default()
        .iter()
        .filter_map(|name| Identifier::read(name).ok())
        .collect()
});

/// `instanceof FallingBlock` (sand, gravel, concrete powder, anvils, dragon egg).
fn is_falling_block(block: Block) -> bool {
    block.data().classes.contains(&"FallingBlock")
}

/// `createRandomShaftPiece(builder, random, footX, footY, footZ, direction, genDepth, type)`.
#[allow(clippy::too_many_arguments)]
fn create_random_shaft_piece(
    builder: &StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    gen_depth: i32,
    mineshaft_type: MineshaftType,
) -> Option<Box<dyn StructurePiece>> {
    let random_selection = random.next_int_bounded(100);
    if random_selection >= 80 {
        let crossing_box = MineShaftCrossing::find_crossing(builder, random, foot_x, foot_y, foot_z, direction)?;
        Some(Box::new(MineShaftCrossing::new(gen_depth, crossing_box, direction, mineshaft_type)))
    } else if random_selection >= 70 {
        let stairs_box = MineShaftStairs::find_stairs(builder, foot_x, foot_y, foot_z, direction)?;
        Some(Box::new(MineShaftStairs::new(gen_depth, stairs_box, direction, mineshaft_type)))
    } else {
        let corridor_box = MineShaftCorridor::find_corridor_size(builder, random, foot_x, foot_y, foot_z, direction)?;
        Some(Box::new(MineShaftCorridor::new(gen_depth, random, corridor_box, direction, mineshaft_type)))
    }
}

/// `generateAndAddPiece(startPiece, builder, random, footX, footY, footZ, direction, depth)`; returns the bounding box
/// of the new piece (the only thing its callers read of it).
#[allow(clippy::too_many_arguments)]
fn generate_and_add_piece(
    start_piece: &mut dyn StructurePiece,
    builder: &mut StructurePiecesBuilder,
    random: &mut dyn RandomSource,
    mineshaft_type: MineshaftType,
    foot_x: i32,
    foot_y: i32,
    foot_z: i32,
    direction: Direction,
    depth: i32,
) -> Option<BoundingBox> {
    if depth > 8 {
        return None;
    }
    let start_box = start_piece.base().bounding_box;
    if (foot_x.wrapping_sub(start_box.min_x())).wrapping_abs() <= 80 && (foot_z.wrapping_sub(start_box.min_z())).wrapping_abs() <= 80 {
        let new_piece = create_random_shaft_piece(builder, random, foot_x, foot_y, foot_z, direction, depth + 1, mineshaft_type)?;
        let bounding_box = new_piece.base().bounding_box;
        builder.add_piece_and_children(new_piece, Some(start_piece), random);
        Some(bounding_box)
    } else {
        None
    }
}

/// `MineShaftPiece`: the shared state (`type`) and helpers of the four pieces.
trait MineShaftPiece: StructurePiece {
    fn mineshaft_type(&self) -> MineshaftType;

    /// `MineShaftPiece.canBeReplaced(level, x, y, z, chunkBB)`.
    fn mineshaft_can_be_replaced(&self, level: &dyn LevelReader, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> bool {
        let block = self.get_block(level, x, y, z, chunk_bb).block();
        let mineshaft_type = self.mineshaft_type();
        block != mineshaft_type.planks() && block != mineshaft_type.wood() && block != mineshaft_type.fence() && block != Block::IRON_CHAIN
    }

    /// `isSupportingBox(level, chunkBB, x0, x1, y1, z0)`.
    fn is_supporting_box(&self, level: &dyn BlockGetter, chunk_bb: &BoundingBox, x0: i32, x1: i32, y1: i32, z0: i32) -> bool {
        (x0..=x1).all(|x| !self.get_block(level, x, y1 + 1, z0, chunk_bb).is_air())
    }

    /// `isInInvalidLocation(level, chunkBB)`.
    fn is_in_invalid_location(&self, level: &dyn WorldGenLevel, chunk_bb: &BoundingBox) -> bool {
        let bounding_box = self.base().bounding_box;
        let x0 = (bounding_box.min_x() - 1).max(chunk_bb.min_x());
        let y0 = (bounding_box.min_y() - 1).max(chunk_bb.min_y());
        let z0 = (bounding_box.min_z() - 1).max(chunk_bb.min_z());
        let x1 = (bounding_box.max_x() + 1).min(chunk_bb.max_x());
        let y1 = (bounding_box.max_y() + 1).min(chunk_bb.max_y());
        let z1 = (bounding_box.max_z() + 1).min(chunk_bb.max_z());
        let center = BlockPos::new((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2);
        if MINESHAFT_BLOCKING.contains(level.env().biomes.key(level.get_biome(center))) {
            return true;
        }
        let liquid = |x: i32, y: i32, z: i32| level.get_block_state(BlockPos::new(x, y, z)).liquid();
        for x in x0..=x1 {
            for z in z0..=z1 {
                if liquid(x, y0, z) || liquid(x, y1, z) {
                    return true;
                }
            }
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if liquid(x, y, z0) || liquid(x, y, z1) {
                    return true;
                }
            }
        }
        for z in z0..=z1 {
            for y in y0..=y1 {
                if liquid(x0, y, z) || liquid(x1, y, z) {
                    return true;
                }
            }
        }
        false
    }

    /// `setPlanksBlock(level, chunkBB, planksBlock, x, y, z)`.
    fn set_planks_block(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, planks_block: BlockState, x: i32, y: i32, z: i32) {
        if self.is_interior(level, x, y, z, chunk_bb) {
            let pos = self.get_world_pos(x, y, z);
            let existing_state = level.get_block_state(pos);
            if !existing_state.is_face_sturdy_at(level, pos, Direction::Up) {
                level.set_block_flags(pos, planks_block, 2);
            }
        }
    }
}

/// `MineShaftPiece(type, tag)`: the piece base and the `MST` type.
fn load_base(piece_type: &'static StructurePieceType, tag: &CompoundTag) -> Result<(PieceBase, MineshaftType), String> {
    Ok((PieceBase::load(piece_type, tag)?, MineshaftType::by_id(tag.get_int_or("MST", 0))))
}

/// `MineShaftCorridor`.
pub struct MineShaftCorridor {
    base: PieceBase,
    mineshaft_type: MineshaftType,
    has_rails: bool,
    spider_corridor: bool,
    has_placed_spider: bool,
    num_sections: i32,
}

#[linkme::distributed_slice(super::super::piece::STRUCTURE_PIECE_TYPES)]
pub static MINE_SHAFT_CORRIDOR: StructurePieceType = StructurePieceType { id: "mscorridor", load: |_, tag| Ok(Box::new(MineShaftCorridor::load(tag)?)) };

impl MineShaftCorridor {
    /// `MineShaftCorridor(tag)`.
    fn load(tag: &CompoundTag) -> Result<MineShaftCorridor, String> {
        let (base, mineshaft_type) = load_base(&MINE_SHAFT_CORRIDOR, tag)?;
        Ok(MineShaftCorridor {
            base,
            mineshaft_type,
            has_rails: tag.get_boolean_or("hr", false),
            spider_corridor: tag.get_boolean_or("sc", false),
            has_placed_spider: tag.get_boolean_or("hps", false),
            num_sections: tag.get_int_or("Num", 0),
        })
    }

    /// `MineShaftCorridor(genDepth, random, boundingBox, direction, type)`.
    pub fn new(gen_depth: i32, random: &mut dyn RandomSource, bounding_box: BoundingBox, direction: Direction, mineshaft_type: MineshaftType) -> MineShaftCorridor {
        let mut base = PieceBase::new(&MINE_SHAFT_CORRIDOR, gen_depth, bounding_box);
        base.set_orientation(Some(direction));
        let has_rails = random.next_int_bounded(3) == 0;
        let spider_corridor = !has_rails && random.next_int_bounded(23) == 0;
        let num_sections = if direction.get_axis() == Axis::Z { bounding_box.get_z_span() / 5 } else { bounding_box.get_x_span() / 5 };
        MineShaftCorridor { base, mineshaft_type, has_rails, spider_corridor, has_placed_spider: false, num_sections }
    }

    /// `findCorridorSize(builder, random, footX, footY, footZ, direction)`.
    pub fn find_corridor_size(
        builder: &StructurePiecesBuilder,
        random: &mut dyn RandomSource,
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        direction: Direction,
    ) -> Option<BoundingBox> {
        let mut corridor_length = random.next_int_bounded(3) + 2;
        while corridor_length > 0 {
            let block_length = corridor_length * 5;
            let bounding_box = match direction {
                Direction::South => BoundingBox::new(0, 0, 0, 2, 2, block_length - 1),
                Direction::West => BoundingBox::new(-(block_length - 1), 0, 0, 0, 2, 2),
                Direction::East => BoundingBox::new(0, 0, 0, block_length - 1, 2, 2),
                _ => BoundingBox::new(0, 0, -(block_length - 1), 2, 2, 0),
            }
            .moved(foot_x, foot_y, foot_z);
            if builder.find_collision_piece(&bounding_box).is_none() {
                return Some(bounding_box);
            }
            corridor_length -= 1;
        }
        None
    }

    /// `placeDoubleLowerOrUpperSupport(level, chunkBB, x, y, z)`.
    fn place_double_lower_or_upper_support(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, x: i32, y: i32, z: i32) {
        let wood_block = self.mineshaft_type.wood().default_state();
        let plank_block = self.mineshaft_type.planks();
        if self.get_block(level, x, y, z, chunk_bb).block() == plank_block {
            self.fill_pillar_down_or_chain_up(level, wood_block, x, y, z, chunk_bb);
        }
        if self.get_block(level, x + 2, y, z, chunk_bb).block() == plank_block {
            self.fill_pillar_down_or_chain_up(level, wood_block, x + 2, y, z, chunk_bb);
        }
    }

    /// `fillPillarDownOrChainUp(level, pillarState, x, y, z, chunkBB)`.
    fn fill_pillar_down_or_chain_up(&self, level: &mut dyn WorldGenLevel, pillar_state: BlockState, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) {
        let pos = self.get_world_pos(x, y, z);
        if !chunk_bb.is_inside(&pos) {
            return;
        }
        let world_y = pos.y();
        let mut distance_from_world_y = 1;
        let mut check_below = true;
        let mut check_above = true;
        while check_below || check_above {
            if check_below {
                let below_pos = BlockPos::new(pos.x(), world_y - distance_from_world_y, pos.z());
                let below_state = level.get_block_state(below_pos);
                let empty_below = self.is_replaceable_by_structures(below_state) && below_state.block() != Block::LAVA;
                if !empty_below && below_state.is_face_sturdy_at(level, below_pos, Direction::Up) {
                    fill_column_between(level, pillar_state, below_pos, world_y - distance_from_world_y + 1, world_y);
                    return;
                }
                check_below = distance_from_world_y <= 20 && empty_below && below_pos.y() > level.get_min_y() + 1;
            }
            if check_above {
                let above_pos = BlockPos::new(pos.x(), world_y + distance_from_world_y, pos.z());
                let above_state = level.get_block_state(above_pos);
                let empty_above = self.is_replaceable_by_structures(above_state);
                if !empty_above && can_support_center(level, above_pos, Direction::Down) && !is_falling_block(above_state.block()) {
                    let fence_pos = BlockPos::new(pos.x(), world_y + 1, pos.z());
                    level.set_block_flags(fence_pos, self.mineshaft_type.fence().default_state(), 2);
                    fill_column_between(level, Block::IRON_CHAIN.default_state(), fence_pos, world_y + 2, world_y + distance_from_world_y);
                    return;
                }
                check_above = distance_from_world_y <= 50 && empty_above && above_pos.y() < level.get_max_y();
            }
            distance_from_world_y += 1;
        }
    }

    /// `placeSupport(level, chunkBB, x0, y0, z, y1, x1, random)`.
    #[allow(clippy::too_many_arguments)]
    fn place_support(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, x0: i32, y0: i32, z: i32, y1: i32, x1: i32, random: &mut dyn RandomSource) {
        if self.is_supporting_box(level, chunk_bb, x0, x1, y1, z) {
            let planks_block = self.mineshaft_type.planks().default_state();
            let fence_block = self.mineshaft_type.fence().default_state();
            self.generate_box(level, chunk_bb, x0, y0, z, x0, y1 - 1, z, fence_block.set(props::WEST, true), cave_air(), false);
            self.generate_box(level, chunk_bb, x1, y0, z, x1, y1 - 1, z, fence_block.set(props::EAST, true), cave_air(), false);
            if random.next_int_bounded(4) == 0 {
                self.generate_box(level, chunk_bb, x0, y1, z, x0, y1, z, planks_block, cave_air(), false);
                self.generate_box(level, chunk_bb, x1, y1, z, x1, y1, z, planks_block, cave_air(), false);
            } else {
                self.generate_box(level, chunk_bb, x0, y1, z, x1, y1, z, planks_block, cave_air(), false);
                self.maybe_generate_block(level, chunk_bb, random, 0.05, x0 + 1, y1, z - 1, Block::WALL_TORCH.default_state().set(props::HORIZONTAL_FACING, Direction::South));
                self.maybe_generate_block(level, chunk_bb, random, 0.05, x0 + 1, y1, z + 1, Block::WALL_TORCH.default_state().set(props::HORIZONTAL_FACING, Direction::North));
            }
        }
    }

    /// `maybePlaceCobWeb(level, chunkBB, random, probability, x, y, z)`.
    #[allow(clippy::too_many_arguments)]
    fn maybe_place_cob_web(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, random: &mut dyn RandomSource, probability: f32, x: i32, y: i32, z: i32) {
        if self.is_interior(level, x, y, z, chunk_bb) && random.next_float() < probability && self.has_sturdy_neighbours(level, chunk_bb, x, y, z, 2) {
            self.place_block(level, Block::COBWEB.default_state(), x, y, z, chunk_bb);
        }
    }

    /// `hasSturdyNeighbours(level, chunkBB, x, y, z, count)`.
    fn has_sturdy_neighbours(&self, level: &dyn WorldGenLevel, chunk_bb: &BoundingBox, x: i32, y: i32, z: i32, count: i32) -> bool {
        let world_pos = self.get_world_pos(x, y, z);
        let mut sturdy_neighbours = 0;
        for direction in Direction::ALL {
            let neighbour = world_pos.relative(direction);
            if chunk_bb.is_inside(&neighbour) && level.get_block_state(neighbour).is_face_sturdy_at(level, neighbour, direction.get_opposite()) {
                sturdy_neighbours += 1;
                if sturdy_neighbours >= count {
                    return true;
                }
            }
        }
        false
    }
}

/// `fillColumnBetween(level, pillarState, pos, bottomInclusive, topExclusive)`.
fn fill_column_between(level: &mut dyn WorldGenLevel, pillar_state: BlockState, pos: BlockPos, bottom_inclusive: i32, top_exclusive: i32) {
    for pillar_y in bottom_inclusive..top_exclusive {
        level.set_block_flags(BlockPos::new(pos.x(), pillar_y, pos.z()), pillar_state, 2);
    }
}

/// `ChestMinecart` as `ProtoChunk.addEntity` saves it right after `setInitialPos`/`setLootTable` (the UUID is random in
/// vanilla too).
fn chest_minecart_tag(pos: BlockPos, loot_table: &str, loot_table_seed: i64) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", "minecraft:chest_minecart");
    let mut position = ListTag::new();
    for v in [f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 0.5, f64::from(pos.z()) + 0.5] {
        position.push(v);
    }
    tag.put("Pos", position);
    let mut motion = ListTag::new();
    for _ in 0..3 {
        motion.push(0.0f64);
    }
    tag.put("Motion", motion);
    let mut rotation = ListTag::new();
    rotation.push(0.0f32);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_double("fall_distance", 0.0);
    tag.put_short("Fire", 0);
    tag.put_short("Air", 300);
    tag.put_boolean("OnGround", false);
    tag.put_boolean("Invulnerable", false);
    tag.put_int("PortalCooldown", 0);
    let mut random = LegacyRandomSource::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64));
    let uuid = mth::create_insecure_uuid(&mut random);
    tag.put_int_array("UUID", mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec());
    tag.put_boolean("FlippedRotation", false);
    tag.put_boolean("HasTicked", true);
    tag.put_string("LootTable", loot_table);
    if loot_table_seed != 0 {
        tag.put_long("LootTableSeed", loot_table_seed);
    }
    tag
}

impl MineShaftPiece for MineShaftCorridor {
    fn mineshaft_type(&self) -> MineshaftType {
        self.mineshaft_type
    }
}

impl StructurePiece for MineShaftCorridor {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn can_be_replaced(&self, level: &dyn LevelReader, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> bool {
        self.mineshaft_can_be_replaced(level, x, y, z, chunk_bb)
    }

    fn add_additional_save_data(&self, _context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_int("MST", self.mineshaft_type.ordinal());
        tag.put_boolean("hr", self.has_rails);
        tag.put_boolean("sc", self.spider_corridor);
        tag.put_boolean("hps", self.has_placed_spider);
        tag.put_int("Num", self.num_sections);
    }

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let depth = self.get_gen_depth();
        let orientation = self.get_orientation();
        let bb = self.base.bounding_box;
        let mineshaft_type = self.mineshaft_type;
        let start: &mut dyn StructurePiece = match start_piece {
            Some(start) => start,
            None => self,
        };
        let end_selection = random.next_int_bounded(4);
        let spawn = |start: &mut dyn StructurePiece, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource, x: i32, y: i32, z: i32, direction: Direction, depth: i32| {
            generate_and_add_piece(start, builder, random, mineshaft_type, x, y, z, direction, depth);
        };
        if let Some(orientation) = orientation {
            // (x, z, direction) of the three exits: straight ahead, and the two side exits chosen by `end_selection`.
            let (ahead, left, right) = match orientation {
                Direction::South => (
                    (bb.min_x(), bb.max_z() + 1, orientation),
                    (bb.min_x() - 1, bb.max_z() - 3, Direction::West),
                    (bb.max_x() + 1, bb.max_z() - 3, Direction::East),
                ),
                Direction::West => (
                    (bb.min_x() - 1, bb.min_z(), orientation),
                    (bb.min_x(), bb.min_z() - 1, Direction::North),
                    (bb.min_x(), bb.max_z() + 1, Direction::South),
                ),
                Direction::East => (
                    (bb.max_x() + 1, bb.min_z(), orientation),
                    (bb.max_x() - 3, bb.min_z() - 1, Direction::North),
                    (bb.max_x() - 3, bb.max_z() + 1, Direction::South),
                ),
                _ => (
                    (bb.min_x(), bb.min_z() - 1, orientation),
                    (bb.min_x() - 1, bb.min_z(), Direction::West),
                    (bb.max_x() + 1, bb.min_z(), Direction::East),
                ),
            };
            let (x, z, direction) = if end_selection <= 1 {
                ahead
            } else if end_selection == 2 {
                left
            } else {
                right
            };
            let y = bb.min_y() - 1 + random.next_int_bounded(3);
            spawn(&mut *start, builder, random, x, y, z, direction, depth);
        }
        if depth < 8 {
            if orientation != Some(Direction::North) && orientation != Some(Direction::South) {
                let mut x = bb.min_x() + 3;
                while x + 3 <= bb.max_x() {
                    let selection = random.next_int_bounded(5);
                    if selection == 0 {
                        spawn(&mut *start, builder, random, x, bb.min_y(), bb.min_z() - 1, Direction::North, depth + 1);
                    } else if selection == 1 {
                        spawn(&mut *start, builder, random, x, bb.min_y(), bb.max_z() + 1, Direction::South, depth + 1);
                    }
                    x += 5;
                }
            } else {
                let mut z = bb.min_z() + 3;
                while z + 3 <= bb.max_z() {
                    let selection = random.next_int_bounded(5);
                    if selection == 0 {
                        spawn(&mut *start, builder, random, bb.min_x() - 1, bb.min_y(), z, Direction::West, depth + 1);
                    } else if selection == 1 {
                        spawn(&mut *start, builder, random, bb.max_x() + 1, bb.min_y(), z, Direction::East, depth + 1);
                    }
                    z += 5;
                }
            }
        }
    }

    /// `MineShaftCorridor.createChest`: a chest minecart on a new rail instead of a chest block.
    fn create_chest(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, random: &mut dyn RandomSource, x: i32, y: i32, z: i32, loot_table: &str) -> bool {
        let pos = self.get_world_pos(x, y, z);
        if chunk_bb.is_inside(&pos) && level.get_block_state(pos).is_air() && !level.get_block_state(pos.below()).is_air() {
            let shape = if random.next_boolean() { RailShape::NorthSouth } else { RailShape::EastWest };
            let state = Block::RAIL.default_state().set(props::RAIL_SHAPE, shape);
            self.place_block(level, state, x, y, z, chunk_bb);
            let seed = random.next_long();
            level.add_fresh_entity_nbt(chest_minecart_tag(pos, loot_table, seed));
            true
        } else {
            false
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
        if self.is_in_invalid_location(level, chunk_bb) {
            return;
        }
        let length = self.num_sections * 5 - 1;
        let planks = self.mineshaft_type.planks().default_state();
        self.generate_box(level, chunk_bb, 0, 0, 0, 2, 1, length, cave_air(), cave_air(), false);
        self.generate_maybe_box(level, chunk_bb, random, 0.8, 0, 2, 0, 2, 2, length, cave_air(), cave_air(), false, false);
        if self.spider_corridor {
            self.generate_maybe_box(level, chunk_bb, random, 0.6, 0, 0, 0, 2, 1, length, Block::COBWEB.default_state(), cave_air(), false, true);
        }
        for section in 0..self.num_sections {
            let z = 2 + section * 5;
            self.place_support(level, chunk_bb, 0, 0, z, 2, 2, random);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.1, 0, 2, z - 1);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.1, 2, 2, z - 1);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.1, 0, 2, z + 1);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.1, 2, 2, z + 1);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.05, 0, 2, z - 2);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.05, 2, 2, z - 2);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.05, 0, 2, z + 2);
            self.maybe_place_cob_web(level, chunk_bb, random, 0.05, 2, 2, z + 2);
            if random.next_int_bounded(100) == 0 {
                self.create_chest(level, chunk_bb, random, 2, 0, z - 1, ABANDONED_MINESHAFT);
            }
            if random.next_int_bounded(100) == 0 {
                self.create_chest(level, chunk_bb, random, 0, 0, z + 1, ABANDONED_MINESHAFT);
            }
            if self.spider_corridor && !self.has_placed_spider {
                let new_z = z - 1 + random.next_int_bounded(3);
                let pos = self.get_world_pos(1, 0, new_z);
                if chunk_bb.is_inside(&pos) && self.is_interior(level, 1, 0, new_z, chunk_bb) {
                    self.has_placed_spider = true;
                    level.set_block_flags(pos, Block::SPAWNER.default_state(), 2);
                    if level.get_block_state(pos).block() == Block::SPAWNER {
                        level.set_block_entity_nbt(spawner_tag(pos, "minecraft:cave_spider"));
                    }
                }
            }
        }
        for x in 0..=2 {
            for z in 0..=length {
                self.set_planks_block(level, chunk_bb, planks, x, -1, z);
            }
        }
        self.place_double_lower_or_upper_support(level, chunk_bb, 0, -1, 2);
        if self.num_sections > 1 {
            let last_support_pillar = length - 2;
            self.place_double_lower_or_upper_support(level, chunk_bb, 0, -1, last_support_pillar);
        }
        if self.has_rails {
            let state = Block::RAIL.default_state().set(props::RAIL_SHAPE, RailShape::NorthSouth);
            for z in 0..=length {
                let floor = self.get_block(level, 1, -1, z, chunk_bb);
                if !floor.is_air() && floor.is_solid_render() {
                    let probability = if self.is_interior(level, 1, 0, z, chunk_bb) { 0.7 } else { 0.9 };
                    self.maybe_generate_block(level, chunk_bb, random, probability, 1, 0, z, state);
                }
            }
        }
    }
}

/// `MineShaftCrossing`.
pub struct MineShaftCrossing {
    base: PieceBase,
    mineshaft_type: MineshaftType,
    direction: Direction,
    is_two_floored: bool,
}

#[linkme::distributed_slice(super::super::piece::STRUCTURE_PIECE_TYPES)]
pub static MINE_SHAFT_CROSSING: StructurePieceType = StructurePieceType { id: "mscrossing", load: |_, tag| Ok(Box::new(MineShaftCrossing::load(tag)?)) };

impl MineShaftCrossing {
    /// `MineShaftCrossing(tag)`: `D` is `Direction.LEGACY_ID_CODEC_2D` (any number tag), `SOUTH` when absent/invalid.
    fn load(tag: &CompoundTag) -> Result<MineShaftCrossing, String> {
        let (base, mineshaft_type) = load_base(&MINE_SHAFT_CROSSING, tag)?;
        Ok(MineShaftCrossing {
            base,
            mineshaft_type,
            is_two_floored: tag.get_boolean_or("tf", false),
            direction: tag.get_byte("D").map_or(Direction::South, |d| Direction::from_2d_data_value(i32::from(d))),
        })
    }

    /// `MineShaftCrossing(genDepth, boundingBox, direction, type)` (no `setOrientation`: the piece has none).
    pub fn new(gen_depth: i32, bounding_box: BoundingBox, direction: Direction, mineshaft_type: MineshaftType) -> MineShaftCrossing {
        MineShaftCrossing { base: PieceBase::new(&MINE_SHAFT_CROSSING, gen_depth, bounding_box), mineshaft_type, direction, is_two_floored: bounding_box.get_y_span() > 3 }
    }

    /// `findCrossing(builder, random, footX, footY, footZ, direction)`.
    pub fn find_crossing(builder: &StructurePiecesBuilder, random: &mut dyn RandomSource, foot_x: i32, foot_y: i32, foot_z: i32, direction: Direction) -> Option<BoundingBox> {
        let y1 = if random.next_int_bounded(4) == 0 { 6 } else { 2 };
        let bounding_box = match direction {
            Direction::South => BoundingBox::new(-1, 0, 0, 3, y1, 4),
            Direction::West => BoundingBox::new(-4, 0, -1, 0, y1, 3),
            Direction::East => BoundingBox::new(0, 0, -1, 4, y1, 3),
            _ => BoundingBox::new(-1, 0, -4, 3, y1, 0),
        }
        .moved(foot_x, foot_y, foot_z);
        if builder.find_collision_piece(&bounding_box).is_some() { None } else { Some(bounding_box) }
    }

    /// `placeSupportPillar(level, chunkBB, x, y0, z, y1)`.
    fn place_support_pillar(&self, level: &mut dyn WorldGenLevel, chunk_bb: &BoundingBox, x: i32, y0: i32, z: i32, y1: i32) {
        if !self.get_block(level, x, y1 + 1, z, chunk_bb).is_air() {
            self.generate_box(level, chunk_bb, x, y0, z, x, y1, z, self.mineshaft_type.planks().default_state(), cave_air(), false);
        }
    }
}

impl MineShaftPiece for MineShaftCrossing {
    fn mineshaft_type(&self) -> MineshaftType {
        self.mineshaft_type
    }
}

impl StructurePiece for MineShaftCrossing {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn can_be_replaced(&self, level: &dyn LevelReader, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> bool {
        self.mineshaft_can_be_replaced(level, x, y, z, chunk_bb)
    }

    fn add_additional_save_data(&self, _context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_int("MST", self.mineshaft_type.ordinal());
        tag.put_boolean("tf", self.is_two_floored);
        tag.put_byte("D", self.direction.get_2d_data_value() as i8);
    }

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let depth = self.get_gen_depth();
        let bb = self.base.bounding_box;
        let mineshaft_type = self.mineshaft_type;
        let direction = self.direction;
        let is_two_floored = self.is_two_floored;
        let start: &mut dyn StructurePiece = match start_piece {
            Some(start) => start,
            None => self,
        };
        let mut spawn = |builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource, x: i32, y: i32, z: i32, direction: Direction| {
            generate_and_add_piece(&mut *start, builder, random, mineshaft_type, x, y, z, direction, depth);
        };
        match direction {
            Direction::South => {
                spawn(builder, random, bb.min_x() + 1, bb.min_y(), bb.max_z() + 1, Direction::South);
                spawn(builder, random, bb.min_x() - 1, bb.min_y(), bb.min_z() + 1, Direction::West);
                spawn(builder, random, bb.max_x() + 1, bb.min_y(), bb.min_z() + 1, Direction::East);
            }
            Direction::West => {
                spawn(builder, random, bb.min_x() + 1, bb.min_y(), bb.min_z() - 1, Direction::North);
                spawn(builder, random, bb.min_x() + 1, bb.min_y(), bb.max_z() + 1, Direction::South);
                spawn(builder, random, bb.min_x() - 1, bb.min_y(), bb.min_z() + 1, Direction::West);
            }
            Direction::East => {
                spawn(builder, random, bb.min_x() + 1, bb.min_y(), bb.min_z() - 1, Direction::North);
                spawn(builder, random, bb.min_x() + 1, bb.min_y(), bb.max_z() + 1, Direction::South);
                spawn(builder, random, bb.max_x() + 1, bb.min_y(), bb.min_z() + 1, Direction::East);
            }
            _ => {
                spawn(builder, random, bb.min_x() + 1, bb.min_y(), bb.min_z() - 1, Direction::North);
                spawn(builder, random, bb.min_x() - 1, bb.min_y(), bb.min_z() + 1, Direction::West);
                spawn(builder, random, bb.max_x() + 1, bb.min_y(), bb.min_z() + 1, Direction::East);
            }
        }
        if is_two_floored {
            if random.next_boolean() {
                spawn(builder, random, bb.min_x() + 1, bb.min_y() + 3 + 1, bb.min_z() - 1, Direction::North);
            }
            if random.next_boolean() {
                spawn(builder, random, bb.min_x() - 1, bb.min_y() + 3 + 1, bb.min_z() + 1, Direction::West);
            }
            if random.next_boolean() {
                spawn(builder, random, bb.max_x() + 1, bb.min_y() + 3 + 1, bb.min_z() + 1, Direction::East);
            }
            if random.next_boolean() {
                spawn(builder, random, bb.min_x() + 1, bb.min_y() + 3 + 1, bb.max_z() + 1, Direction::South);
            }
        }
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
        if self.is_in_invalid_location(level, chunk_bb) {
            return;
        }
        let bb = self.base.bounding_box;
        let planks = self.mineshaft_type.planks().default_state();
        let air = cave_air();
        if self.is_two_floored {
            self.generate_box(level, chunk_bb, bb.min_x() + 1, bb.min_y(), bb.min_z(), bb.max_x() - 1, bb.min_y() + 3 - 1, bb.max_z(), air, air, false);
            self.generate_box(level, chunk_bb, bb.min_x(), bb.min_y(), bb.min_z() + 1, bb.max_x(), bb.min_y() + 3 - 1, bb.max_z() - 1, air, air, false);
            self.generate_box(level, chunk_bb, bb.min_x() + 1, bb.max_y() - 2, bb.min_z(), bb.max_x() - 1, bb.max_y(), bb.max_z(), air, air, false);
            self.generate_box(level, chunk_bb, bb.min_x(), bb.max_y() - 2, bb.min_z() + 1, bb.max_x(), bb.max_y(), bb.max_z() - 1, air, air, false);
            self.generate_box(level, chunk_bb, bb.min_x() + 1, bb.min_y() + 3, bb.min_z() + 1, bb.max_x() - 1, bb.min_y() + 3, bb.max_z() - 1, air, air, false);
        } else {
            self.generate_box(level, chunk_bb, bb.min_x() + 1, bb.min_y(), bb.min_z(), bb.max_x() - 1, bb.max_y(), bb.max_z(), air, air, false);
            self.generate_box(level, chunk_bb, bb.min_x(), bb.min_y(), bb.min_z() + 1, bb.max_x(), bb.max_y(), bb.max_z() - 1, air, air, false);
        }
        self.place_support_pillar(level, chunk_bb, bb.min_x() + 1, bb.min_y(), bb.min_z() + 1, bb.max_y());
        self.place_support_pillar(level, chunk_bb, bb.min_x() + 1, bb.min_y(), bb.max_z() - 1, bb.max_y());
        self.place_support_pillar(level, chunk_bb, bb.max_x() - 1, bb.min_y(), bb.min_z() + 1, bb.max_y());
        self.place_support_pillar(level, chunk_bb, bb.max_x() - 1, bb.min_y(), bb.max_z() - 1, bb.max_y());
        let y = bb.min_y() - 1;
        for x in bb.min_x()..=bb.max_x() {
            for z in bb.min_z()..=bb.max_z() {
                self.set_planks_block(level, chunk_bb, planks, x, y, z);
            }
        }
    }
}

/// `MineShaftRoom`.
pub struct MineShaftRoom {
    base: PieceBase,
    mineshaft_type: MineshaftType,
    child_entrance_boxes: Vec<BoundingBox>,
}

#[linkme::distributed_slice(super::super::piece::STRUCTURE_PIECE_TYPES)]
pub static MINE_SHAFT_ROOM: StructurePieceType = StructurePieceType { id: "msroom", load: |_, tag| Ok(Box::new(MineShaftRoom::load(tag)?)) };

impl MineShaftRoom {
    /// `MineShaftRoom(tag)`: `Entrances` is `BoundingBox.CODEC.listOf()` (empty when absent or malformed).
    fn load(tag: &CompoundTag) -> Result<MineShaftRoom, String> {
        let (base, mineshaft_type) = load_base(&MINE_SHAFT_ROOM, tag)?;
        let child_entrance_boxes = tag
            .get_list("Entrances")
            .and_then(|list| list.iter().map(|entry| read_bounding_box(Some(entry))).collect::<Option<Vec<_>>>())
            .unwrap_or_default();
        Ok(MineShaftRoom { base, mineshaft_type, child_entrance_boxes })
    }

    /// `MineShaftRoom(genDepth, random, west, north, type)`.
    pub fn new(gen_depth: i32, random: &mut dyn RandomSource, west: i32, north: i32, mineshaft_type: MineshaftType) -> MineShaftRoom {
        let max_x = west + 7 + random.next_int_bounded(6);
        let max_y = 54 + random.next_int_bounded(6);
        let max_z = north + 7 + random.next_int_bounded(6);
        MineShaftRoom {
            base: PieceBase::new(&MINE_SHAFT_ROOM, gen_depth, BoundingBox::new(west, 50, north, max_x, max_y, max_z)),
            mineshaft_type,
            child_entrance_boxes: Vec::new(),
        }
    }
}

impl MineShaftPiece for MineShaftRoom {
    fn mineshaft_type(&self) -> MineshaftType {
        self.mineshaft_type
    }
}

impl StructurePiece for MineShaftRoom {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn can_be_replaced(&self, level: &dyn LevelReader, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> bool {
        self.mineshaft_can_be_replaced(level, x, y, z, chunk_bb)
    }

    fn add_additional_save_data(&self, _context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_int("MST", self.mineshaft_type.ordinal());
        let mut entrances = ListTag::new();
        for entrance in &self.child_entrance_boxes {
            entrances.push(Tag::IntArray(bounding_box_to_ints(entrance)));
        }
        tag.put("Entrances", entrances);
    }

    /// The room is always the start piece (`startPiece == this`), so `start_piece` is `None` by contract.
    fn add_children(&mut self, _start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let depth = self.get_gen_depth();
        let bb = self.base.bounding_box;
        let mineshaft_type = self.mineshaft_type;
        let mut height_space = bb.get_y_span() - 3 - 1;
        if height_space <= 0 {
            height_space = 1;
        }

        let mut pos = 0;
        while pos < bb.get_x_span() {
            pos += random.next_int_bounded(bb.get_x_span());
            if pos + 3 > bb.get_x_span() {
                break;
            }
            let y = bb.min_y() + random.next_int_bounded(height_space) + 1;
            if let Some(child_box) = generate_and_add_piece(self, builder, random, mineshaft_type, bb.min_x() + pos, y, bb.min_z() - 1, Direction::North, depth) {
                self.child_entrance_boxes.push(BoundingBox::new(child_box.min_x(), child_box.min_y(), bb.min_z(), child_box.max_x(), child_box.max_y(), bb.min_z() + 1));
            }
            pos += 4;
        }

        pos = 0;
        while pos < bb.get_x_span() {
            pos += random.next_int_bounded(bb.get_x_span());
            if pos + 3 > bb.get_x_span() {
                break;
            }
            let y = bb.min_y() + random.next_int_bounded(height_space) + 1;
            if let Some(child_box) = generate_and_add_piece(self, builder, random, mineshaft_type, bb.min_x() + pos, y, bb.max_z() + 1, Direction::South, depth) {
                self.child_entrance_boxes.push(BoundingBox::new(child_box.min_x(), child_box.min_y(), bb.max_z() - 1, child_box.max_x(), child_box.max_y(), bb.max_z()));
            }
            pos += 4;
        }

        pos = 0;
        while pos < bb.get_z_span() {
            pos += random.next_int_bounded(bb.get_z_span());
            if pos + 3 > bb.get_z_span() {
                break;
            }
            let y = bb.min_y() + random.next_int_bounded(height_space) + 1;
            if let Some(child_box) = generate_and_add_piece(self, builder, random, mineshaft_type, bb.min_x() - 1, y, bb.min_z() + pos, Direction::West, depth) {
                self.child_entrance_boxes.push(BoundingBox::new(bb.min_x(), child_box.min_y(), child_box.min_z(), bb.min_x() + 1, child_box.max_y(), child_box.max_z()));
            }
            pos += 4;
        }

        pos = 0;
        while pos < bb.get_z_span() {
            pos += random.next_int_bounded(bb.get_z_span());
            if pos + 3 > bb.get_z_span() {
                break;
            }
            let y = bb.min_y() + random.next_int_bounded(height_space) + 1;
            if let Some(child_box) = generate_and_add_piece(self, builder, random, mineshaft_type, bb.max_x() + 1, y, bb.min_z() + pos, Direction::East, depth) {
                self.child_entrance_boxes.push(BoundingBox::new(bb.max_x() - 1, child_box.min_y(), child_box.min_z(), bb.max_x(), child_box.max_y(), child_box.max_z()));
            }
            pos += 4;
        }
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
        if self.is_in_invalid_location(level, chunk_bb) {
            return;
        }
        let bb = self.base.bounding_box;
        let air = cave_air();
        self.generate_box(level, chunk_bb, bb.min_x(), bb.min_y() + 1, bb.min_z(), bb.max_x(), (bb.min_y() + 3).min(bb.max_y()), bb.max_z(), air, air, false);
        for entrance_box in &self.child_entrance_boxes {
            self.generate_box(
                level,
                chunk_bb,
                entrance_box.min_x(),
                entrance_box.max_y() - 2,
                entrance_box.min_z(),
                entrance_box.max_x(),
                entrance_box.max_y(),
                entrance_box.max_z(),
                air,
                air,
                false,
            );
        }
        self.generate_upper_half_sphere(level, chunk_bb, bb.min_x(), bb.min_y() + 4, bb.min_z(), bb.max_x(), bb.max_y(), bb.max_z(), air, false);
    }

    /// `MineShaftRoom.move`: also moves the entrance boxes.
    fn move_by(&mut self, dx: i32, dy: i32, dz: i32) {
        self.base.bounding_box.move_(dx, dy, dz);
        for entrance_box in &mut self.child_entrance_boxes {
            entrance_box.move_(dx, dy, dz);
        }
    }
}

/// `MineShaftStairs`.
pub struct MineShaftStairs {
    base: PieceBase,
    mineshaft_type: MineshaftType,
}

#[linkme::distributed_slice(super::super::piece::STRUCTURE_PIECE_TYPES)]
pub static MINE_SHAFT_STAIRS: StructurePieceType = StructurePieceType { id: "msstairs", load: |_, tag| Ok(Box::new(MineShaftStairs::load(tag)?)) };

impl MineShaftStairs {
    /// `MineShaftStairs(tag)`.
    fn load(tag: &CompoundTag) -> Result<MineShaftStairs, String> {
        let (base, mineshaft_type) = load_base(&MINE_SHAFT_STAIRS, tag)?;
        Ok(MineShaftStairs { base, mineshaft_type })
    }

    /// `MineShaftStairs(genDepth, boundingBox, direction, type)`.
    pub fn new(gen_depth: i32, bounding_box: BoundingBox, direction: Direction, mineshaft_type: MineshaftType) -> MineShaftStairs {
        let mut base = PieceBase::new(&MINE_SHAFT_STAIRS, gen_depth, bounding_box);
        base.set_orientation(Some(direction));
        MineShaftStairs { base, mineshaft_type }
    }

    /// `findStairs(builder, random, footX, footY, footZ, direction)`.
    pub fn find_stairs(builder: &StructurePiecesBuilder, foot_x: i32, foot_y: i32, foot_z: i32, direction: Direction) -> Option<BoundingBox> {
        let bounding_box = match direction {
            Direction::South => BoundingBox::new(0, -5, 0, 2, 2, 8),
            Direction::West => BoundingBox::new(-8, -5, 0, 0, 2, 2),
            Direction::East => BoundingBox::new(0, -5, 0, 8, 2, 2),
            _ => BoundingBox::new(0, -5, -8, 2, 2, 0),
        }
        .moved(foot_x, foot_y, foot_z);
        if builder.find_collision_piece(&bounding_box).is_some() { None } else { Some(bounding_box) }
    }
}

impl MineShaftPiece for MineShaftStairs {
    fn mineshaft_type(&self) -> MineshaftType {
        self.mineshaft_type
    }
}

impl StructurePiece for MineShaftStairs {
    fn base(&self) -> &PieceBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PieceBase {
        &mut self.base
    }

    fn can_be_replaced(&self, level: &dyn LevelReader, x: i32, y: i32, z: i32, chunk_bb: &BoundingBox) -> bool {
        self.mineshaft_can_be_replaced(level, x, y, z, chunk_bb)
    }

    fn add_additional_save_data(&self, _context: &StructurePieceSerializationContext<'_>, tag: &mut CompoundTag) {
        tag.put_int("MST", self.mineshaft_type.ordinal());
    }

    fn add_children(&mut self, start_piece: Option<&mut dyn StructurePiece>, builder: &mut StructurePiecesBuilder, random: &mut dyn RandomSource) {
        let depth = self.get_gen_depth();
        let bb = self.base.bounding_box;
        let mineshaft_type = self.mineshaft_type;
        let orientation = self.get_orientation();
        let start: &mut dyn StructurePiece = match start_piece {
            Some(start) => start,
            None => self,
        };
        if let Some(orientation) = orientation {
            let (x, z) = match orientation {
                Direction::South => (bb.min_x(), bb.max_z() + 1),
                Direction::West => (bb.min_x() - 1, bb.min_z()),
                Direction::East => (bb.max_x() + 1, bb.min_z()),
                _ => (bb.min_x(), bb.min_z() - 1),
            };
            let direction = if matches!(orientation, Direction::South | Direction::West | Direction::East) { orientation } else { Direction::North };
            generate_and_add_piece(start, builder, random, mineshaft_type, x, bb.min_y(), z, direction, depth);
        }
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
        if self.is_in_invalid_location(level, chunk_bb) {
            return;
        }
        let air = cave_air();
        self.generate_box(level, chunk_bb, 0, 5, 0, 2, 7, 1, air, air, false);
        self.generate_box(level, chunk_bb, 0, 0, 7, 2, 2, 8, air, air, false);
        for i in 0..5 {
            self.generate_box(level, chunk_bb, 0, 5 - i - i32::from(i < 4), 2 + i, 2, 7 - i, 2 + i, air, air, false);
        }
    }
}
