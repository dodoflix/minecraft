//! `net.minecraft.world.level.block.WebBlock`.
use super::prelude::*;
use mc_core::phys::Vec3;

pub struct WebBlock;

impl BlockBehaviour for WebBlock {
    fn entity_inside(
        &self,
        state: BlockState,
        _level: &mut crate::server::level::ServerLevel,
        _pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        // `MobEffects.WEAVING` (0.5, 0.25, 0.5) needs the effects slice (`Entity::has_effect`)
        entity.make_stuck_in_block(state, Vec3::new(0.25, f64::from(0.05f32), 0.25));
    }
}

register_block_class!("WebBlock", |_| Box::new(WebBlock));
