//! `MonsterRoomFeature` (`monster_room`): dungeons.

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature, safe_set_block};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::block_entity_data::{get_block_entity_nbt, set_block_entity_loot_table, spawner_tag};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::BlockGetter;
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use std::sync::Arc;

/// `MOBS`.
const MOBS: [&str; 4] = ["minecraft:skeleton", "minecraft:zombie", "minecraft:zombie", "minecraft:spider"];

pub struct MonsterRoomFeature;

impl MonsterRoomFeature {
    pub fn decode(_fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(MonsterRoomFeature))
    }
}

/// `StructurePiece.reorient(level, pos, state)`: turns a chest to face away from a single solid neighbour.
fn reorient(level: &dyn BlockGetter, block_pos: BlockPos, block_state: BlockState) -> BlockState {
    let mut solid_neighbor: Option<Direction> = None;
    for direction in Plane::Horizontal {
        let state = level.get_block_state(block_pos.relative(direction));
        if state.is_block(Block::CHEST) {
            return block_state;
        }
        if state.is_solid_render() {
            if solid_neighbor.is_some() {
                solid_neighbor = None;
                break;
            }
            solid_neighbor = Some(direction);
        }
    }
    if let Some(solid) = solid_neighbor {
        return block_state.set(props::HORIZONTAL_FACING, solid.get_opposite());
    }
    let mut lock_dir = block_state.get(props::HORIZONTAL_FACING);
    let mut relative_pos = block_pos.relative(lock_dir);
    if level.get_block_state(relative_pos).is_solid_render() {
        lock_dir = lock_dir.get_opposite();
        relative_pos = block_pos.relative(lock_dir);
    }
    if level.get_block_state(relative_pos).is_solid_render() {
        lock_dir = lock_dir.get_clockwise();
        relative_pos = block_pos.relative(lock_dir);
    }
    if level.get_block_state(relative_pos).is_solid_render() {
        lock_dir = lock_dir.get_opposite();
    }
    block_state.set(props::HORIZONTAL_FACING, lock_dir)
}

impl Feature for MonsterRoomFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let air = Block::CAVE_AIR.default_state();
        let replaceable_tag = |s: BlockState| !s.is_tag("features_cannot_replace");
        let xr = random.next_int_bounded(2) + 2;
        let min_x = -xr - 1;
        let max_x = xr + 1;
        let zr = random.next_int_bounded(2) + 2;
        let min_z = -zr - 1;
        let max_z = zr + 1;
        let mut hole_count = 0;

        for dx in min_x..=max_x {
            for dy in -1..=4 {
                for dz in min_z..=max_z {
                    let hole_pos = origin.offset(dx, dy, dz);
                    let solid = level.get_block_state(hole_pos).is_solid();
                    if dy == -1 && !solid {
                        return false;
                    }
                    if dy == 4 && !solid {
                        return false;
                    }
                    if (dx == min_x || dx == max_x || dz == min_z || dz == max_z) && dy == 0 && level.is_empty_block(hole_pos) && level.is_empty_block(hole_pos.above()) {
                        hole_count += 1;
                    }
                }
            }
        }

        if !(1..=5).contains(&hole_count) {
            return false;
        }

        for dx in min_x..=max_x {
            for dy in (-1..=3).rev() {
                for dz in min_z..=max_z {
                    let wall_block = origin.offset(dx, dy, dz);
                    let wall_state = level.get_block_state(wall_block);
                    if dx == min_x || dy == -1 || dz == min_z || dx == max_x || dy == 4 || dz == max_z {
                        if wall_block.y() >= level.get_min_y() && !level.get_block_state(wall_block.below()).is_solid() {
                            level.set_block(wall_block, air, 2, 512);
                        } else if wall_state.is_solid() && !wall_state.is_block(Block::CHEST) {
                            if dy == -1 && random.next_int_bounded(4) != 0 {
                                safe_set_block(level, wall_block, Block::MOSSY_COBBLESTONE.default_state(), replaceable_tag);
                            } else {
                                safe_set_block(level, wall_block, Block::COBBLESTONE.default_state(), replaceable_tag);
                            }
                        }
                    } else if !wall_state.is_block(Block::CHEST) && !wall_state.is_block(Block::SPAWNER) {
                        safe_set_block(level, wall_block, air, replaceable_tag);
                    }
                }
            }
        }

        for _ in 0..2 {
            for _ in 0..3 {
                let xc = origin.x() + random.next_int_bounded(xr * 2 + 1) - xr;
                let yc = origin.y();
                let zc = origin.z() + random.next_int_bounded(zr * 2 + 1) - zr;
                let chest_pos = BlockPos::new(xc, yc, zc);
                if level.is_empty_block(chest_pos) {
                    let mut wall_count = 0;
                    for direction in Plane::Horizontal {
                        if level.get_block_state(chest_pos.relative(direction)).is_solid() {
                            wall_count += 1;
                        }
                    }
                    if wall_count == 1 {
                        let chest = reorient(level, chest_pos, Block::CHEST.default_state());
                        safe_set_block(level, chest_pos, chest, replaceable_tag);
                        set_block_entity_loot_table(level, random, chest_pos, "minecraft:chests/simple_dungeon");
                        break;
                    }
                }
            }
        }

        safe_set_block(level, origin, Block::SPAWNER.default_state(), replaceable_tag);
        if get_block_entity_nbt(level, origin).is_some_and(|tag| tag.get_string("id") == Some("minecraft:mob_spawner")) {
            let entity = MOBS[random.next_int_bounded(MOBS.len() as i32) as usize];
            level.set_block_entity_nbt(spawner_tag(origin, entity));
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static MONSTER_ROOM: TypeEntry<dyn Feature> = TypeEntry { name: "monster_room", decode: MonsterRoomFeature::decode };
