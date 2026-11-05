//! `net.minecraft.world.level.block.WitherRoseBlock` (extends `FlowerBlock`).
use super::flower_block::FlowerBlock;
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour_methods};
use crate::world::entity::effect::MobEffectInstance;
use mc_data::MobEffect;
use mc_protocol::components::SuspiciousStewEffects;

pub struct WitherRoseBlock(FlowerBlock);

impl MayPlaceOn for WitherRoseBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_wither_rose")
    }
}

impl BlockBehaviour for WitherRoseBlock {
    vegetation_behaviour_methods!();
    fn get_suspicious_effects(&self) -> Option<SuspiciousStewEffects> {
        self.0.get_suspicious_effects()
    }
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut crate::server::level::ServerLevel,
        _pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        use crate::server::level::level_data::ServerLevelData;
        if level.level_data.get_difficulty() != mc_world::level_storage::Difficulty::Peaceful && entity.living().is_some() {
            let source = level.damage_sources().wither();
            if !entity.is_invulnerable_to(level, &source) {
                entity.add_effect(level, MobEffectInstance::new(MobEffect::WITHER, 40, 0, false, true, true, None), None);
            }
        }
    }
    /// `getBeeInteractionEffect`: `new MobEffectInstance(MobEffects.WITHER, 40)`.
    fn get_bee_interaction_effect(&self) -> Option<MobEffectInstance> {
        Some(MobEffectInstance::new(MobEffect::WITHER, 40, 0, false, true, true, None))
    }
}

register_block_class!("WitherRoseBlock", |block| Box::new(WitherRoseBlock(FlowerBlock::new(block))));
