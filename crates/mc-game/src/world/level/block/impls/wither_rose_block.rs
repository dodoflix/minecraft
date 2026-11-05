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
    /// `getBeeInteractionEffect`: `new MobEffectInstance(MobEffects.WITHER, 40)`.
    fn get_bee_interaction_effect(&self) -> Option<MobEffectInstance> {
        Some(MobEffectInstance::new(MobEffect::WITHER, 40, 0, false, true, true, None))
    }
}

register_block_class!("WitherRoseBlock", |block| Box::new(WitherRoseBlock(FlowerBlock::new(block))));
