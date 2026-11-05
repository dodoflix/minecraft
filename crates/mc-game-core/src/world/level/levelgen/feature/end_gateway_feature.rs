//! `EndGatewayFeature` (`end_gateway`).

use super::{FEATURE_TYPES, Feature, set_block};
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::WorldGenLevel;
use crate::world::level::levelgen::block_entity_data::{end_gateway_tag, get_block_entity_nbt};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_block_pos, parse_bool};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use std::sync::Arc;

pub struct EndGatewayFeature {
    pub exit: Option<BlockPos>,
    pub exact: bool,
}

impl EndGatewayFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(EndGatewayFeature { exit: fields.optional("exit", parse_block_pos)?, exact: fields.required("exact", parse_bool)? }))
    }
}

impl Feature for EndGatewayFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        for pos in BlockPos::between_closed(&origin.offset(-1, -2, -1), &origin.offset(1, 2, 1)) {
            let same_x = pos.x() == origin.x();
            let same_y = pos.y() == origin.y();
            let same_z = pos.z() == origin.z();
            let end = (pos.y() - origin.y()).abs() == 2;
            if same_x && same_y && same_z {
                set_block(level, pos, Block::END_GATEWAY.default_state());
                // `level.getBlockEntity(pos)` materializes the DUMMY entry; without an exit it is never fetched and stays DUMMY
                if let Some(target_pos) = self.exit {
                    if get_block_entity_nbt(level, pos).is_some_and(|tag| tag.get_string("id") == Some("minecraft:end_gateway")) {
                        level.set_block_entity_nbt(end_gateway_tag(pos, Some((target_pos, self.exact))));
                    }
                }
            } else if same_y {
                set_block(level, pos, Block::AIR.default_state());
            } else if end && same_x && same_z {
                set_block(level, pos, Block::BEDROCK.default_state());
            } else if (same_x || same_z) && !end {
                set_block(level, pos, Block::BEDROCK.default_state());
            } else {
                set_block(level, pos, Block::AIR.default_state());
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static END_GATEWAY: TypeEntry<dyn Feature> = TypeEntry { name: "end_gateway", decode: EndGatewayFeature::decode };
