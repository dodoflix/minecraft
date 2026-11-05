//! `net.minecraft.world.level.block.GlowLichenBlock` (extends `MultifaceSpreadeableBlock`; the shape logic is `MultifaceBlock`'s).
use super::multiface_spreader::MultifaceSpreader;
use super::prelude::*;
use std::sync::LazyLock;

use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct GlowLichenBlock;

static SPREADER: LazyLock<MultifaceSpreader> = LazyLock::new(|| MultifaceSpreader::new(Block::GLOW_LICHEN));

/// `GlowLichenBlock.getSpreader()`.
pub fn spreader() -> &'static MultifaceSpreader {
    &SPREADER
}

impl BlockBehaviour for GlowLichenBlock {
    crate::simple_waterlogged!();
    // `MultifaceBlock` shape logic
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        super::multiface_block::MultifaceBlock.update_shape(state, level, pos, direction, neighbour_pos, neighbour_state)
    });
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        super::multiface_block::MultifaceBlock.can_survive(state, level, pos)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        super::multiface_block::MultifaceBlock.rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        super::multiface_block::MultifaceBlock.mirror(state, mirror)
    }
    fn can_be_replaced(&self, state: BlockState, context: &crate::world::item::BlockPlaceContext) -> bool {
        super::multiface_block::MultifaceBlock.can_be_replaced(state, context)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        super::multiface_block::MultifaceBlock.get_state_for_placement(block, level, context)
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        Direction::ALL.into_iter().any(|face| spreader().can_spread_in_any_direction(state, level, pos, face.get_opposite()))
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        spreader().spread_from_random_face_toward_random_direction_with_level_random(state, level, pos);
    }
}

register_block_class!("GlowLichenBlock", |_| Box::new(GlowLichenBlock));
