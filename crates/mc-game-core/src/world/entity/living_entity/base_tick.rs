//! `LivingEntity.baseTick()`: counters, drowning/air supply, death animation.

use super::util::{self, FluidKind, has_effect, is_eye_in_fluid, l, lm};
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, RemovalReason};
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::BlockGetter;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{Attribute, MobEffect};

/// `MobEffectUtil.hasWaterBreathing(mob)`.
fn has_water_breathing(entity: &dyn Entity) -> bool {
    has_effect(entity, MobEffect::WATER_BREATHING)
        || has_effect(entity, MobEffect::CONDUIT_POWER)
        || has_effect(entity, MobEffect::BREATH_OF_THE_NAUTILUS)
}

/// `MobEffectUtil.shouldEffectsRefillAirsupply(mob)`.
fn should_effects_refill_airsupply(entity: &dyn Entity) -> bool {
    !has_effect(entity, MobEffect::BREATH_OF_THE_NAUTILUS)
        || has_effect(entity, MobEffect::WATER_BREATHING)
        || has_effect(entity, MobEffect::CONDUIT_POWER)
}

/// `decreaseAirSupply(currentSupply)`: the `OXYGEN_BONUS` (respiration) attribute makes some ticks free.
pub fn decrease_air_supply(entity: &mut dyn Entity, current_supply: i32) -> i32 {
    let oxygen_bonus = lm(entity)
        .attributes
        .get_instance(Attribute::OXYGEN_BONUS)
        .map_or(0.0, |respiration| respiration.get_value());
    if oxygen_bonus > 0.0 && entity.base_mut().random.next_double() >= 1.0 / (oxygen_bonus + 1.0) {
        current_supply
    } else {
        current_supply - 1
    }
}

/// `tickDeath()`.
pub fn tick_death(entity: &mut dyn Entity, level: &mut ServerLevel) {
    lm(entity).death_time += 1;
    if l(entity).death_time >= 20 && !entity.is_removed() {
        util::broadcast_entity_event(level, entity.id(), 60);
        super::effects::trigger_on_death_mob_effects(entity, level, RemovalReason::Killed);
        entity.before_remove(level, RemovalReason::Killed);
        entity.remove(RemovalReason::Killed);
    }
}

/// `LivingEntity.baseTick()`.
pub fn base_tick(entity: &mut dyn Entity, level: &mut ServerLevel) {
    lm(entity).swing_state.tick();
    if entity.base().first_tick
        && let Some(bed) = l(entity).get_sleeping_pos()
        && !super::sleeping::set_pos_to_bed(entity, level, bed)
    {
        super::sleeping::stop_sleeping(entity, level);
    }
    {
        let id = entity.id();
        crate::world::item::enchantment_helper::tick_effects(level, &mut Actors::of(entity), id);
    }
    crate::world::entity::entity::base_tick(entity, level);
    if entity.is_alive() {
        let is_player = entity.is_player();
        if util::is_in_wall(entity, level) {
            util::hurt(level, entity, |sources| sources.in_wall(), 1.0);
        } else if is_player
            && !level
                .world_border()
                .is_within_bounds_aabb(&entity.base().bounding_box())
        {
            let border = level.world_border();
            let dist = border.get_distance_to_border(entity.base().x(), entity.base().z())
                + border.get_safe_zone();
            if dist < 0.0 {
                let damage_per_block = border.get_damage_per_block();
                if damage_per_block > 0.0 {
                    let amount = 1.max(mth::floor_f64(-dist * damage_per_block)) as f32;
                    util::hurt(level, entity, |sources| sources.out_of_border(), amount);
                }
            }
        }
        let eye = entity.base().get_eye_position();
        if is_eye_in_fluid(entity, FluidKind::Water)
            && level
                .get_block_state(BlockPos::containing(eye.x, eye.y, eye.z))
                .block()
                != Block::BUBBLE_COLUMN
        {
            let can_drown_in_water = !entity.can_breathe_underwater()
                && !has_water_breathing(entity)
                && !entity.player().is_some_and(|p| p.abilities.invulnerable);
            if can_drown_in_water {
                let supply = entity.base().get_air_supply();
                let decreased = entity.decrease_air_supply(supply);
                entity.base_mut().set_air_supply(decreased);
                if entity.should_take_drowning_damage() {
                    entity.base_mut().set_air_supply(0);
                    util::broadcast_entity_event(level, entity.id(), 67);
                    util::hurt(level, entity, |sources| sources.drown(), 2.0);
                }
            } else if entity.base().get_air_supply() < entity.get_max_air_supply()
                && should_effects_refill_airsupply(entity)
            {
                let supply = entity.base().get_air_supply();
                let increased = entity.increase_air_supply(supply);
                entity.base_mut().set_air_supply(increased);
            }
            if let Some(vehicle_id) = entity.base().vehicle
                && level
                    .get_entity(vehicle_id)
                    .is_some_and(dismounts_underwater)
            {
                level.stop_riding(entity);
            }
        } else if entity.base().get_air_supply() < entity.get_max_air_supply() {
            let supply = entity.base().get_air_supply();
            let increased = entity.increase_air_supply(supply);
            entity.base_mut().set_air_supply(increased);
        }
        let pos = entity.base().block_position();
        if l(entity).last_pos != Some(pos) {
            lm(entity).last_pos = Some(pos);
            {
                let id = entity.id();
                crate::world::item::enchantment_helper::run_location_changed_effects(
                    level,
                    &mut Actors::of(entity),
                    id,
                );
            }
        }
    }
    if l(entity).hurt_time > 0 {
        lm(entity).hurt_time -= 1;
    }
    if l(entity).damage_cooldown_time > 0 && !entity.is_player() {
        lm(entity).damage_cooldown_time -= 1;
    }
    if entity.is_dead_or_dying() {
        // `level.shouldTickDeath(this)` is true for a `ServerLevel`
        entity.tick_death(level);
    }
    if l(entity).last_hurt_by_player_memory_time > 0 {
        lm(entity).last_hurt_by_player_memory_time -= 1;
    } else {
        lm(entity).last_hurt_by_player = None;
    }
    if let Some(target) = l(entity).last_hurt_mob
        && !level.get_entity(target).is_some_and(|t| t.is_alive())
    {
        lm(entity).last_hurt_mob = None;
    }
    if let Some(uuid) = l(entity).last_hurt_by_mob
        && let Some(hurt_by) = level
            .get_entity_by_uuid(uuid)
            .filter(|e| e.living().is_some())
    {
        let hurt_by_alive = hurt_by.is_alive();
        let tick_count = entity.base().tick_count;
        if !hurt_by_alive || tick_count - l(entity).last_hurt_by_mob_timestamp > 100 {
            let living = lm(entity);
            living.last_hurt_by_mob = None;
            living.last_hurt_by_mob_timestamp = tick_count;
        }
    }
    super::effects::tick_effects(entity, level);
    let living = lm(entity);
    living.y_head_rot_o = living.y_head_rot;
    living.y_body_rot_o = living.y_body_rot;
}

/// `Entity.dismountsUnderwater()` (entity type tag `dismounts_underwater`).
fn dismounts_underwater(vehicle: &dyn Entity) -> bool {
    mc_data::tags::get::<mc_data::EntityType>("dismounts_underwater")
        .is_some_and(|t| t.contains(vehicle.get_type()))
}
