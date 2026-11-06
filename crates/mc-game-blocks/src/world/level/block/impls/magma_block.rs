//! `net.minecraft.world.level.block.MagmaBlock`.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct MagmaBlock;

impl BlockBehaviour for MagmaBlock {
    fn step_on(
        &self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _on_state: BlockState,
        entity: &mut dyn Entity,
    ) {
        if !entity.is_stepping_carefully() && entity.living().is_some() {
            let source = level.damage_sources().hot_floor();
            entity.hurt_server(level, &source, 1.0);
        }
    }
}

register_block_class!("MagmaBlock", |_| Box::new(MagmaBlock));
