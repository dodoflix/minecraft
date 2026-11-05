//! `net.minecraft.world.level.block.BuddingAmethystBlock` (extends `AmethystBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;

const GROWTH_CHANCE: i32 = 5;

pub struct BuddingAmethystBlock;

/// `BuddingAmethystBlock.canClusterGrowAtState`.
pub fn can_cluster_grow_at_state(state: BlockState) -> bool {
    state.is_air() || (state.is_block(Block::WATER) && state.get_fluid_state().is_full())
}

impl BlockBehaviour for BuddingAmethystBlock {
    fn on_projectile_hit(&self, level: &mut crate::server::level::ServerLevel, _state: BlockState, hit: &mc_core::phys::BlockHitResult, _projectile: &mut dyn crate::world::entity::projectile::Projectile) {
        super::amethyst_block::on_projectile_hit(level, hit);
    }
    fn random_tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.random().next_int_bounded(GROWTH_CHANCE) != 0 {
            return;
        }
        let grow_direction = Direction::ALL[level.random().next_int_bounded(Direction::ALL.len() as i32) as usize];
        let grow_pos = pos.relative(grow_direction);
        let relative_state = level.get_block_state(grow_pos);
        let facing_grow = |state: BlockState| state.get(props::FACING) == grow_direction;
        let next_stage = if can_cluster_grow_at_state(relative_state) {
            Some(Block::SMALL_AMETHYST_BUD)
        } else if relative_state.is_block(Block::SMALL_AMETHYST_BUD) && facing_grow(relative_state) {
            Some(Block::MEDIUM_AMETHYST_BUD)
        } else if relative_state.is_block(Block::MEDIUM_AMETHYST_BUD) && facing_grow(relative_state) {
            Some(Block::LARGE_AMETHYST_BUD)
        } else if relative_state.is_block(Block::LARGE_AMETHYST_BUD) && facing_grow(relative_state) {
            Some(Block::AMETHYST_CLUSTER)
        } else {
            None
        };
        if let Some(next_stage) = next_stage {
            let target_state = next_stage
                .default_state()
                .set(props::FACING, grow_direction)
                .set(props::WATERLOGGED, relative_state.get_fluid_state().get_type() == mc_data::fluid::Fluid::WATER);
            level.set_block_and_update(grow_pos, target_state);
        }
    }
}

register_block_class!("BuddingAmethystBlock", |_| Box::new(BuddingAmethystBlock));
