//! `BonusChestFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::block_entity_data::set_block_entity_loot_table;
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::java_util::shuffle;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Plane;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

/// `BuiltInLootTables.SPAWN_BONUS_CHEST`.
const SPAWN_BONUS_CHEST: &str = "minecraft:chests/spawn_bonus_chest";

pub struct BonusChestFeature;

#[linkme::distributed_slice(FEATURE_TYPES)]
static BONUS_CHEST: TypeEntry<dyn Feature> = TypeEntry { name: "bonus_chest", decode: |_, _| Ok(Arc::new(BonusChestFeature)) };

impl Feature for BonusChestFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let chunk_pos = ChunkPos::containing(&origin);
        let mut x_poses: Vec<i32> = (chunk_pos.get_min_block_x()..=chunk_pos.get_max_block_x()).collect();
        shuffle(&mut x_poses, random);
        let mut z_poses: Vec<i32> = (chunk_pos.get_min_block_z()..=chunk_pos.get_max_block_z()).collect();
        shuffle(&mut z_poses, random);

        for &x in &x_poses {
            for &z in &z_poses {
                let chest_pos = BlockPos::new(x, level.get_height_at(HeightmapType::MotionBlockingNoLeaves, x, z), z);
                if level.is_empty_block(chest_pos) || level.get_block_state(chest_pos).get_collision_shape(&*level, chest_pos).is_empty() {
                    level.set_block_flags(chest_pos, Block::CHEST.default_state(), 2);
                    set_block_entity_loot_table(level, random, chest_pos, SPAWN_BONUS_CHEST);
                    let torch = Block::TORCH.default_state();

                    for direction in Plane::Horizontal {
                        let torch_pos = chest_pos.relative(direction);
                        if torch.can_survive(&*level, torch_pos) {
                            level.set_block_flags(torch_pos, torch, 2);
                        }
                    }
                    return true;
                }
            }
        }
        false
    }
}
