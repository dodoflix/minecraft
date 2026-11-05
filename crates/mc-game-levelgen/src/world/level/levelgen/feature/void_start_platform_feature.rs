//! `VoidStartPlatformFeature`.

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::Block;
use std::sync::Arc;

const PLATFORM_OFFSET: BlockPos = BlockPos::new(8, 3, 8);
const PLATFORM_ORIGIN_CHUNK: ChunkPos = ChunkPos::containing(&PLATFORM_OFFSET);

pub struct VoidStartPlatformFeature;

fn checkerboard_distance(xa: i32, za: i32, xb: i32, zb: i32) -> i32 {
    (xa - xb).abs().max((za - zb).abs())
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static VOID_START_PLATFORM: TypeEntry<dyn Feature> = TypeEntry { name: "void_start_platform", decode: |_, _| Ok(Arc::new(VoidStartPlatformFeature)) };

impl Feature for VoidStartPlatformFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let current_chunk_pos = ChunkPos::containing(&origin);
        if checkerboard_distance(current_chunk_pos.x, current_chunk_pos.z, PLATFORM_ORIGIN_CHUNK.x, PLATFORM_ORIGIN_CHUNK.z) > 1 {
            return true;
        }
        let platform_origin = BlockPos::new(PLATFORM_OFFSET.x(), origin.y() + PLATFORM_OFFSET.y(), PLATFORM_OFFSET.z());

        for z in current_chunk_pos.get_min_block_z()..=current_chunk_pos.get_max_block_z() {
            for x in current_chunk_pos.get_min_block_x()..=current_chunk_pos.get_max_block_x() {
                if checkerboard_distance(platform_origin.x(), platform_origin.z(), x, z) <= 16 {
                    let block_pos = BlockPos::new(x, platform_origin.y(), z);
                    if block_pos == platform_origin {
                        level.set_block_flags(block_pos, Block::COBBLESTONE.default_state(), 2);
                    } else {
                        level.set_block_flags(block_pos, Block::STONE.default_state(), 2);
                    }
                }
            }
        }
        true
    }
}
