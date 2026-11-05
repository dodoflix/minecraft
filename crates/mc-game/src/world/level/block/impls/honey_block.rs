//! `net.minecraft.world.level.block.HoneyBlock` (the entity-facing half: sliding and landing).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::InsideBlockEffectApplier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;

pub struct HoneyBlock;

/// `doesEntityDoHoneyBlockSlideEffects`: living entities, minecarts, primed TNT and boats.
fn does_entity_do_honey_block_slide_effects(entity: &dyn Entity) -> bool {
    let name = entity.get_type().data().name;
    entity.living().is_some() || name == "minecraft:tnt" || name.ends_with("minecart") || name.ends_with("boat") || name.ends_with("raft")
}

fn get_old_delta_y(delta_y: f64) -> f64 {
    delta_y / f64::from(0.98f32) + 0.08
}

fn get_new_delta_y(delta_y: f64) -> f64 {
    (delta_y - 0.08) * f64::from(0.98f32)
}

fn is_sliding_down(pos: BlockPos, entity: &dyn Entity) -> bool {
    let b = entity.base();
    if b.on_ground() {
        false
    } else if b.y() > f64::from(pos.y()) + 0.9375 - 1.0E-7 {
        false
    } else if get_old_delta_y(b.delta_movement().y) >= -0.08 {
        false
    } else {
        let dx = (f64::from(pos.x()) + 0.5 - b.x()).abs();
        let dz = (f64::from(pos.z()) + 0.5 - b.z()).abs();
        let overlap_distance = 0.4375 + f64::from(b.get_bb_width() / 2.0);
        dx + 1.0E-7 > overlap_distance || dz + 1.0E-7 > overlap_distance
    }
}

fn do_slide_movement(entity: &mut dyn Entity) {
    let delta_movement = entity.base().delta_movement();
    if get_old_delta_y(delta_movement.y) < -0.13 {
        let horizontal_reduction_factor = -0.05 / get_old_delta_y(delta_movement.y);
        entity.base_mut().set_delta_movement(Vec3::new(delta_movement.x * horizontal_reduction_factor, get_new_delta_y(-0.05), delta_movement.z * horizontal_reduction_factor));
    } else {
        entity.base_mut().set_delta_movement(Vec3::new(delta_movement.x, get_new_delta_y(-0.05), delta_movement.z));
    }
    entity.reset_fall_distance();
}

fn maybe_do_slide_effects(level: &mut ServerLevel, entity: &mut dyn Entity) {
    if does_entity_do_honey_block_slide_effects(entity) {
        if LevelAccessor::random(level).next_int_bounded(5) == 0 {
            entity.play_sound(level, mc_data::SoundEvent::BLOCK_HONEY_BLOCK_SLIDE, 1.0, 1.0);
        }
        if LevelAccessor::random(level).next_int_bounded(5) == 0 {
            level.broadcast_entity_event(entity.id(), 53);
        }
    }
}

impl BlockBehaviour for HoneyBlock {
    fn fall_on(&self, level: &mut ServerLevel, state: BlockState, _pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        entity.play_sound(level, mc_data::SoundEvent::BLOCK_HONEY_BLOCK_SLIDE, 1.0, 1.0);
        level.broadcast_entity_event(entity.id(), 54);
        let source = level.damage_sources().fall();
        if entity.cause_fall_damage(level, fall_distance, 0.2, &source) {
            let sound_type = state.get_sound_type();
            entity.play_sound(level, sound_type.fall_sound, sound_type.volume * 0.5, sound_type.pitch * 0.75);
        }
    }

    fn entity_inside(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        if is_sliding_down(pos, entity) {
            // `maybeDoSlideAchievement` (HONEY_BLOCK_SLIDE trigger) needs the advancement triggers
            do_slide_movement(entity);
            maybe_do_slide_effects(level, entity);
        }
    }
}

register_block_class!("HoneyBlock", |_| Box::new(HoneyBlock));
