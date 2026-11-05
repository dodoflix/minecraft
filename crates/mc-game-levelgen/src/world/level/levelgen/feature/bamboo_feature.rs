//! `BambooFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_float_range};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::BambooLeaves;
use mc_data::props;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct BambooFeature {
    pub probability: f32,
}

impl BambooFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(BambooFeature { probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))? }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static BAMBOO: TypeEntry<dyn Feature> = TypeEntry { name: "bamboo", decode: BambooFeature::decode };

fn bamboo_trunk() -> BlockState {
    Block::BAMBOO.default_state().set(props::AGE_1, 1).set(props::BAMBOO_LEAVES, BambooLeaves::None).set(props::STAGE, 0)
}

impl Feature for BambooFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let trunk = bamboo_trunk();
        let final_large = trunk.set(props::BAMBOO_LEAVES, BambooLeaves::Large).set(props::STAGE, 1);
        let top_large = trunk.set(props::BAMBOO_LEAVES, BambooLeaves::Large);
        let top_small = trunk.set(props::BAMBOO_LEAVES, BambooLeaves::Small);

        let mut placed = 0;
        let mut bamboo_pos = origin;
        if level.is_empty_block(bamboo_pos) {
            if Block::BAMBOO.default_state().can_survive(&*level, bamboo_pos) {
                let height = random.next_int_bounded(12) + 5;
                if random.next_float() < self.probability {
                    let r = random.next_int_bounded(4) + 1;
                    for xx in origin.x() - r..=origin.x() + r {
                        for zz in origin.z() - r..=origin.z() + r {
                            let xd = xx - origin.x();
                            let zd = zz - origin.z();
                            if xd * xd + zd * zd <= r * r {
                                let podzol_pos = BlockPos::new(xx, level.get_height_at(HeightmapType::WorldSurface, xx, zz) - 1, zz);
                                if level.get_block_state(podzol_pos).is_tag("beneath_bamboo_podzol_replaceable") {
                                    level.set_block_flags(podzol_pos, Block::PODZOL.default_state(), 2);
                                }
                            }
                        }
                    }
                }

                let mut i = 0;
                while i < height && level.is_empty_block(bamboo_pos) {
                    level.set_block_flags(bamboo_pos, trunk, 2);
                    bamboo_pos = bamboo_pos.relative(Direction::Up);
                    i += 1;
                }

                if bamboo_pos.y() - origin.y() >= 3 {
                    level.set_block_flags(bamboo_pos, final_large, 2);
                    bamboo_pos = bamboo_pos.relative(Direction::Down);
                    level.set_block_flags(bamboo_pos, top_large, 2);
                    bamboo_pos = bamboo_pos.relative(Direction::Down);
                    level.set_block_flags(bamboo_pos, top_small, 2);
                }
            }
            placed += 1;
        }
        placed > 0
    }
}
