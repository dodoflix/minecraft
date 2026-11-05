//! `LivingEntity.randomTeleport` / `checkPositionAndTeleport` and `TeleportRandomlyConsumeEffect` (chorus fruit).

use super::util::lm;
use crate::registry::keys;
use crate::registry::typed::dimension_type::DimensionType;
use crate::registry::typed::level_stem::{DimensionTypeSource, LevelStem};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::movement::contains_any_liquid;
use crate::world::level::BlockGetter;
use crate::world::level::collision::CollisionGetter;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_protocol::components::TeleportRandomlyConsumeEffect;
use mc_protocol::packets::play::SoundSource;

/// `Level.getLogicalHeight()` of the level's dimension type (the level height when no `LevelStem` is registered).
pub fn get_logical_height(level: &ServerLevel) -> i32 {
    let access = &level.registries;
    let from_stem = || {
        let stems = access.registry(keys::LEVEL_STEM)?;
        let id = stems.id_of(&level.dimension)?;
        let stem = stems.entry(id)?.value.clone()?.downcast::<LevelStem>().ok()?;
        match &stem.dimension_type {
            DimensionTypeSource::Reference(type_id) => access.get::<DimensionType>(keys::DIMENSION_TYPE, type_id).map(|t| t.logical_height),
            DimensionTypeSource::Inline(t) => Some(t.logical_height),
        }
    };
    from_stem().unwrap_or_else(|| level.level_height().height)
}

/// `LivingEntity.randomTeleport(x, y, z, showParticles, avoidanceTag)`.
pub fn random_teleport(entity: &mut dyn Entity, level: &mut ServerLevel, xx: f64, yy: f64, zz: f64, show_particles: bool, avoidance_tag: &str) -> bool {
    let mut y = yy;
    let clamped = level.world_border().clamp_vec3_to_bound(xx, yy, zz);
    let mut pos = BlockPos::containing(clamped.x, clamped.y, clamped.z);
    if crate::world::level::LevelReader::has_chunk(&*level, pos.x() >> 4, pos.z() >> 4) {
        while pos.y() > level.get_min_y() {
            pos = pos.below();
            let state = level.get_block_state(pos);
            if state.block().is_in_tag("entities_can_teleport_to") {
                return check_position_and_teleport(entity, level, clamped.x, y, clamped.z, show_particles, avoidance_tag, state);
            }
            y -= 1.0;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn check_position_and_teleport(entity: &mut dyn Entity, level: &mut ServerLevel, xx: f64, y: f64, zz: f64, show_particles: bool, avoidance_tag: &str, state: BlockState) -> bool {
    let invalid = |s: BlockState| s.block().is_in_tag(avoidance_tag);
    if invalid(state) {
        return false;
    }
    let aabb = entity.make_bounding_box(Vec3::new(xx, y, zz));
    if !(level.no_collision(None, None, &aabb) && !contains_any_liquid(&*level, &aabb)) {
        return false;
    }
    if BlockPos::between_closed_aabb(&aabb).any(|p| invalid(level.get_block_state(p))) {
        return false;
    }
    // `canRandomlyTeleportTo(x, y, z)`: true for every entity but a few mobs
    entity.teleport_to(level, xx, y, zz);
    if show_particles {
        super::util::broadcast_entity_event(level, entity.id(), 46);
    }
    true
}

/// `TeleportRandomlyConsumeEffect.apply(level, stack, user)`.
pub fn apply_teleport_randomly(effect: &TeleportRandomlyConsumeEffect, level: &mut ServerLevel, user: &mut dyn Entity) -> bool {
    let diameter = f64::from(effect.diameter);
    for _ in 0..16 {
        let xx = user.base().x() + (user.base_mut().random.next_double() - 0.5) * diameter;
        let yy = mth::clamp_f64(
            user.base().y() + (user.base_mut().random.next_double() - 0.5) * diameter,
            f64::from(level.get_min_y()),
            f64::from(level.get_min_y() + get_logical_height(level) - 1),
        );
        let zz = user.base().z() + (user.base_mut().random.next_double() - 0.5) * diameter;
        if user.base().is_passenger() {
            level.stop_riding(user);
        }
        let old_pos = user.base().position();
        if user.random_teleport(level, xx, yy, zz, true, "consumable_does_not_teleport_to") {
            level.game_event_from_entity_ref(&*user, GameEvent::TELEPORT, old_pos);
            let (sound, source) = if user.get_type() == EntityType::FOX {
                (SoundEvent::ENTITY_FOX_TELEPORT, SoundSource::Neutral)
            } else {
                (SoundEvent::ITEM_CHORUS_FRUIT_TELEPORT, SoundSource::Players)
            };
            let p = user.base().position();
            level.play_sound_at(None, p.x, p.y, p.z, sound, source, 1.0, 1.0);
            if effect.directional_particles {
                let origin = BlockPos::containing(old_pos.x, old_pos.y, old_pos.z);
                let target = user.base().block_position();
                crate::world::level::LevelAccessor::level_event(level, None, 2017, origin, clamped_pack_difference_in_position(origin, target, 127, 127, 127));
            }
            user.reset_fall_distance();
            reset_current_impulse_context(user);
            return true;
        }
    }
    false
}

/// `resetCurrentImpulseContext()`.
pub fn reset_current_impulse_context(entity: &mut dyn Entity) {
    let living = lm(entity);
    living.current_impulse_context_reset_grace_time = 0;
    living.current_impulse_impact_pos = None;
}

/// `BlockUtil.clampedPackDifferenceInPosition(pos, testPos, xRadius, yRadius, zRadius)`.
pub fn clamped_pack_difference_in_position(pos: BlockPos, test_pos: BlockPos, x_radius: i32, y_radius: i32, z_radius: i32) -> i32 {
    let (xr, yr, zr) = (x_radius.clamp(0, 127), y_radius.clamp(0, 127), z_radius.clamp(0, 127));
    let x_diff = (test_pos.x() - pos.x()).clamp(-127, 127);
    let y_diff = (test_pos.y() - pos.y()).clamp(-127, 127);
    let z_diff = (test_pos.z() - pos.z()).clamp(-127, 127);
    ((x_diff + xr) & 0xFF) << 16 | ((y_diff + yr) & 0xFF) << 8 | ((z_diff + zr) & 0xFF)
}
