//! `net.minecraft.world.level.block.AmethystClusterBlock` (extends `AmethystBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct AmethystClusterBlock;

impl BlockBehaviour for AmethystClusterBlock {
    mc_game_core::simple_waterlogged!();
    fn on_projectile_hit(&self, level: &mut mc_game_core::server::level::ServerLevel, _state: BlockState, hit: &mc_core::phys::BlockHitResult, _projectile: &mut dyn mc_game_core::world::entity::projectile::Projectile) {
        super::amethyst_block::on_projectile_hit(level, hit);
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = state.get(props::FACING);
        let adjacent_pos = pos.relative(direction.get_opposite());
        level.get_block_state(adjacent_pos).is_face_sturdy_at(level, adjacent_pos, direction)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction == state.get(props::FACING).get_opposite() && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn get_state_for_placement(&self, block: Block, level: &mut mc_game_core::server::level::ServerLevel, context: &mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        Some(block.default_state().set(props::WATERLOGGED, level.get_fluid_state(pos).get_type() == mc_data::fluid::Fluid::WATER).set(props::FACING, context.get_clicked_face()))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("AmethystClusterBlock", |_| Box::new(AmethystClusterBlock));
