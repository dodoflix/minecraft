//! `net.minecraft.world.level.block.SweetBerryBushBlock` (`entityInside`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::InsideBlockEffectApplier;
use mc_core::phys::Vec3;

pub struct SweetBerryBushBlock;

impl BlockBehaviour for SweetBerryBushBlock {
    fn entity_inside(&self, state: BlockState, level: &mut ServerLevel, _pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        use mc_data::EntityType;
        if entity.living().is_some() && entity.get_type() != EntityType::FOX && entity.get_type() != EntityType::BEE {
            entity.make_stuck_in_block(state, Vec3::new(f64::from(0.8f32), 0.75, f64::from(0.8f32)));
            if state.get(props::AGE_3) != 0 {
                let movement = if entity.is_client_authoritative() { entity.get_known_movement(level) } else { entity.base().old_position().subtract(entity.base().position()) };
                if movement.horizontal_distance_sqr() > 0.0 {
                    let (xs, zs) = (movement.x.abs(), movement.z.abs());
                    if xs >= f64::from(0.003f32) || zs >= f64::from(0.003f32) {
                        let source = level.damage_sources().sweet_berry_bush();
                        entity.hurt_server(level, &source, 1.0);
                    }
                }
            }
        }
    }
}

register_block_class!("SweetBerryBushBlock", |_| Box::new(SweetBerryBushBlock));
