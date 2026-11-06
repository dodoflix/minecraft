//! `ReplaceBlobsFeature` (`netherrack_replace_blobs`): replaces every block of one type within a random manhattan blob.

use mc_core::direction::{Axis, Direction};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_block_state};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::Feature;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct ReplaceBlobsFeature {
    pub target_state: BlockState,
    pub replace_state: BlockState,
    pub radius: IntProvider,
}

impl Feature for ReplaceBlobsFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let target_block = self.target_state.block();
        let mut clamped = origin.mutable();
        clamped.clamp(Axis::Y, level.get_min_y() + 1, level.get_max_y());
        let Some(center_pos) = find_target(level, clamped.immutable(), target_block) else {
            return false;
        };
        let radius_x = self.radius.sample(random);
        let radius_y = self.radius.sample(random);
        let radius_z = self.radius.sample(random);
        let maximum_radius = radius_x.max(radius_y.max(radius_z));
        let mut replaced_any = false;

        for pos in
            BlockPos::within_box_by_manhattan_distance(&center_pos, radius_x, radius_y, radius_z)
        {
            if pos.dist_manhattan(&center_pos) > maximum_radius {
                break;
            }
            if level.get_block_state(pos).is_block(target_block) {
                mc_game_core::world::level::levelgen::feature::set_block(
                    level,
                    pos,
                    self.replace_state,
                );
                replaced_any = true;
            }
        }
        replaced_any
    }
}

fn find_target(level: &dyn WorldGenLevel, mut cursor: BlockPos, target: Block) -> Option<BlockPos> {
    while cursor.y() > level.get_min_y() + 1 {
        if level.get_block_state(cursor).is_block(target) {
            return Some(cursor);
        }
        cursor = cursor.relative(Direction::Down);
    }
    None
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static NETHERRACK_REPLACE_BLOBS: TypeEntry<dyn Feature> = TypeEntry {
    name: "netherrack_replace_blobs",
    decode: |fields, _| {
        Ok(Arc::new(ReplaceBlobsFeature {
            target_state: fields.required("target", parse_block_state)?,
            replace_state: fields.required("state", parse_block_state)?,
            radius: fields.required("radius", |v| IntProvider::from_json_range(v, 0, 12))?,
        }))
    },
};
