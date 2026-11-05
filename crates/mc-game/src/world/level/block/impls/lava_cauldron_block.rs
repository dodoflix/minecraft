//! `net.minecraft.world.level.block.LavaCauldronBlock` (`entityInside`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::{InsideBlockEffectApplier, InsideBlockEffectType};

pub struct LavaCauldronBlock;

impl BlockBehaviour for LavaCauldronBlock {
    fn entity_inside(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _entity: &mut dyn Entity, effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        effect_applier.apply(InsideBlockEffectType::ClearFreeze);
        effect_applier.apply(InsideBlockEffectType::LavaIgnite);
        effect_applier.run_after(InsideBlockEffectType::LavaIgnite, Box::new(|entity, level| entity.lava_hurt(level)));
    }
}

register_block_class!("LavaCauldronBlock", |_| Box::new(LavaCauldronBlock));
