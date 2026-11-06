//! `EndPodiumFeature` (`end_podium`): the exit portal podium.

use mc_core::direction::Plane;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::props;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_bool};
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature, set_block};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

pub struct EndPodiumFeature {
    pub active: bool,
}

impl EndPodiumFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(EndPodiumFeature {
            active: fields.optional("active", parse_bool)?.unwrap_or(false),
        }))
    }

    /// `dropPreviousAndSetBlock(level, pos, block)`.
    fn drop_previous_and_set_block(level: &mut dyn WorldGenLevel, pos: BlockPos, block: Block) {
        if !level.get_block_state(pos).is_block(block) {
            level.destroy_block(pos, true, None, 512);
            set_block(level, pos, block.default_state());
        }
    }
}

impl Feature for EndPodiumFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        for pos in BlockPos::between_closed(
            &BlockPos::new(origin.x() - 4, origin.y() - 1, origin.z() - 4),
            &BlockPos::new(origin.x() + 4, origin.y() + 32, origin.z() + 4),
        ) {
            let inside_rim = pos.closer_than(&origin, 2.5);
            if inside_rim || pos.closer_than(&origin, 3.5) {
                if pos.y() < origin.y() {
                    if inside_rim {
                        set_block(level, pos, Block::BEDROCK.default_state());
                    } else if self.active {
                        Self::drop_previous_and_set_block(level, pos, Block::END_STONE);
                    } else {
                        set_block(level, pos, Block::END_STONE.default_state());
                    }
                } else if pos.y() > origin.y() {
                    if self.active {
                        Self::drop_previous_and_set_block(level, pos, Block::AIR);
                    } else {
                        set_block(level, pos, Block::AIR.default_state());
                    }
                } else if !inside_rim {
                    set_block(level, pos, Block::BEDROCK.default_state());
                } else if self.active {
                    Self::drop_previous_and_set_block(level, pos, Block::END_PORTAL);
                } else {
                    set_block(level, pos, Block::AIR.default_state());
                }
            }
        }

        for y in 0..4 {
            set_block(level, origin.above_n(y), Block::BEDROCK.default_state());
        }
        let center_of_pillar = origin.above_n(2);
        for face in Plane::Horizontal {
            set_block(
                level,
                center_of_pillar.relative(face),
                Block::WALL_TORCH
                    .default_state()
                    .set(props::HORIZONTAL_FACING, face),
            );
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static END_PODIUM: TypeEntry<dyn Feature> = TypeEntry {
    name: "end_podium",
    decode: EndPodiumFeature::decode,
};
