//! `SingleBlockPillarFeature`: a column of one block growing in a vertical direction, optionally capped by another feature.

use super::Feature;
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{LoadError, TypeEntry, parse_direction, parse_float_range};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::placement::PlacedFeature;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct SingleBlockPillarFeature {
    pub block: Holder<dyn BlockStateProvider>,
    pub can_replace: Arc<dyn BlockPredicate>,
    pub direction: Direction,
    pub chance_to_continue: f32,
    pub cap_feature: Option<Holder<PlacedFeature>>,
}

/// `BlockPredicate.alwaysTrue()`.
struct AlwaysTrue;

impl BlockPredicate for AlwaysTrue {
    fn test(&self, _level: &dyn LevelAccessor, _pos: BlockPos) -> bool {
        true
    }
}

impl Feature for SingleBlockPillarFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let mut pos = origin;
        while self.can_replace.test(&*level, pos) && random.next_float() < self.chance_to_continue && !level.is_outside_build_height(pos.y()) {
            let state = self.block.value().get_state(&*level, random, pos);
            level.set_block_flags(pos, state, 2);
            pos = pos.relative(self.direction);
        }
        pos = pos.relative(self.direction.get_opposite());
        if let Some(cap) = &self.cap_feature {
            cap.value().place(level, generator, random, pos);
        }
        true
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static SINGLE_BLOCK_PILLAR: TypeEntry<dyn Feature> = TypeEntry {
    name: "single_block_pillar",
    decode: |fields, ctx| {
        Ok(Arc::new(SingleBlockPillarFeature {
            block: fields.required("block", |v| ctx.block_state_provider(v))?,
            can_replace: fields
                .optional("can_replace", |v| blockpredicates::decode(v, ctx))?
                .unwrap_or_else(|| Arc::new(AlwaysTrue) as Arc<dyn BlockPredicate>),
            direction: fields.required("direction", |v| {
                let direction = parse_direction(v)?;
                if direction.get_axis().is_vertical() { Ok(direction) } else { Err(LoadError::new("Expected a vertical direction")) }
            })?,
            chance_to_continue: fields.optional("chance_to_continue", |v| parse_float_range(v, 0.0, 1.0))?.unwrap_or(1.0),
            cap_feature: fields.optional("cap_feature", |v| ctx.placed_feature(v))?,
        }))
    },
};
