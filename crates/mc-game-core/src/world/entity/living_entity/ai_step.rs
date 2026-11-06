//! `LivingEntity.aiStep()`: jump input, travel, freezing, pushing.

use super::travel::{is_in_shallow_fluid, travel_ridden};
use super::util::{self, FluidKind, has_effect, is_in_lava, is_in_water, l, lm};
use crate::server::level::ServerLevel;
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use crate::world::entity::{Entity, movement};
use crate::world::level::BlockGetter;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::{Attribute, MobEffect};
use std::sync::LazyLock;

/// `SPEED_MODIFIER_POWDER_SNOW_ID`.
static SPEED_MODIFIER_POWDER_SNOW_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::new("minecraft", "powder_snow").expect("valid id"));

/// `removeFrost()`.
fn remove_frost(entity: &mut dyn Entity) {
    if let Some(speed) = lm(entity)
        .attributes
        .get_instance(Attribute::MOVEMENT_SPEED)
    {
        if speed.get_modifier(&SPEED_MODIFIER_POWDER_SNOW_ID).is_some() {
            speed.remove_modifier(&SPEED_MODIFIER_POWDER_SNOW_ID);
        }
    }
}

/// `tryAddFrost()`.
fn try_add_frost(entity: &mut dyn Entity, level: &ServerLevel) {
    if level
        .get_block_state(movement::get_on_pos_legacy(entity, level))
        .is_air()
    {
        return;
    }
    let ticks_frozen = entity.base().get_ticks_frozen();
    if ticks_frozen > 0 {
        let required = entity.get_ticks_required_to_freeze();
        let percent_frozen = ticks_frozen.min(required) as f32 / required as f32;
        let slow_amount = -0.05f32 * percent_frozen;
        let Some(speed) = lm(entity)
            .attributes
            .get_instance(Attribute::MOVEMENT_SPEED)
        else {
            return;
        };
        speed.add_transient_modifier(AttributeModifier {
            id: SPEED_MODIFIER_POWDER_SNOW_ID.clone(),
            amount: f64::from(slow_amount),
            operation: AttributeModifierOperation::AddValue,
        });
    }
}

/// `checkAutoSpinAttack(old, current)`.
fn check_auto_spin_attack(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    old: &mc_core::phys::AABB,
    current: &mc_core::phys::AABB,
) {
    let minmax = old.minmax(current);
    let entities = level.get_entities(Some(entity.id()), &minmax, |_| true);
    if !entities.is_empty() {
        for id in entities {
            let is_living = level.get_entity(id).is_some_and(|e| e.living().is_some());
            if is_living {
                level.with_entity(id, |level, other| {
                    entity.do_auto_attack_on_touch(level, other)
                });
                lm(entity).auto_spin_attack_ticks = 0;
                let dm = entity.base().delta_movement().scale(-0.2);
                entity.base_mut().set_delta_movement(dm);
                break;
            }
        }
    } else if entity.base().horizontal_collision {
        lm(entity).auto_spin_attack_ticks = 0;
    }
    if l(entity).auto_spin_attack_ticks <= 0 {
        use crate::world::entity::living::LIVING_ENTITY_FLAG_SPIN_ATTACK;
        let living = lm(entity);
        living.set_living_entity_flag(LIVING_ENTITY_FLAG_SPIN_ATTACK, false);
        living.auto_spin_attack_dmg = 0.0;
        living.auto_spin_attack_item_stack = None;
    }
}

/// `LivingEntity.aiStep()`.
pub fn ai_step(entity: &mut dyn Entity, level: &mut ServerLevel) {
    if l(entity).no_jump_delay > 0 {
        lm(entity).no_jump_delay -= 1;
    }
    if !entity.can_simulate_movement() {
        let dm = entity.base().delta_movement().scale(0.98);
        entity.base_mut().set_delta_movement(dm);
    }
    if l(entity).lerp_head_steps > 0 {
        let living = lm(entity);
        // `lerpHeadRotationStep(steps, target)`
        living.y_head_rot = mth::rot_lerp_f64(
            1.0 / f64::from(living.lerp_head_steps),
            f64::from(living.y_head_rot),
            living.lerp_y_head_rot,
        ) as f32;
        living.lerp_head_steps -= 1;
    }
    entity.tick_equipment(level);
    let movement = entity.base().delta_movement();
    let (mut dx, mut dy, mut dz) = (movement.x, movement.y, movement.z);
    if entity.get_type() == mc_data::EntityType::PLAYER {
        if movement.horizontal_distance_sqr() < 9.0E-6 {
            dx = 0.0;
            dz = 0.0;
        }
    } else {
        if movement.x.abs() < 0.003 {
            dx = 0.0;
        }
        if movement.z.abs() < 0.003 {
            dz = 0.0;
        }
    }
    if movement.y.abs() < 0.003 {
        dy = 0.0;
    }
    entity.base_mut().set_delta_movement_xyz(dx, dy, dz);
    entity.apply_input();
    if entity.is_immobile() {
        let living = lm(entity);
        living.jumping = false;
        living.xxa = 0.0;
        living.zza = 0.0;
    } else if entity.is_effective_ai() {
        entity.server_ai_step(level);
    }
    // jump
    if l(entity).jumping && entity.is_affected_by_fluids() {
        let fluid_height = if is_in_lava(entity) {
            util::get_fluid_height(entity, FluidKind::Lava)
        } else {
            util::get_fluid_height(entity, FluidKind::Water)
        };
        let in_water_and_has_fluid_height = is_in_water(entity) && fluid_height > 0.0;
        let fluid_jump_threshold = entity.get_fluid_jump_threshold();
        let on_ground = entity.base().on_ground();
        if !in_water_and_has_fluid_height || on_ground && !(fluid_height > fluid_jump_threshold) {
            if !is_in_lava(entity) || on_ground && is_in_shallow_fluid(entity, FluidKind::Lava) {
                if (on_ground
                    || in_water_and_has_fluid_height && fluid_height <= fluid_jump_threshold)
                    && l(entity).no_jump_delay == 0
                {
                    entity.jump_from_ground(level);
                    lm(entity).no_jump_delay = 10;
                }
            } else {
                entity.jump_in_liquid(FluidKind::Lava);
            }
        } else {
            entity.jump_in_liquid(FluidKind::Water);
        }
    } else {
        lm(entity).no_jump_delay = 0;
    }
    // travel
    if l(entity).is_fall_flying() {
        entity.update_fall_flying(level);
    }
    let before_travel_box = entity.base().bounding_box();
    let input = Vec3::new(
        f64::from(l(entity).xxa),
        f64::from(l(entity).yya),
        f64::from(l(entity).zza),
    );
    if has_effect(entity, MobEffect::SLOW_FALLING) || has_effect(entity, MobEffect::LEVITATION) {
        entity.reset_fall_distance();
    }
    let controller = entity
        .get_controlling_passenger()
        .filter(|&id| level.get_entity(id).is_some_and(|e| e.is_player()));
    if let (Some(controller), true) = (controller, entity.is_alive()) {
        travel_ridden(entity, level, controller, input);
    } else if entity.can_simulate_movement() && entity.is_effective_ai() {
        entity.travel(level, input);
    }
    entity.apply_effects_from_blocks(level);
    // freezing
    if !entity.base().is_in_powder_snow || !entity.can_freeze() {
        let frozen = (entity.base().get_ticks_frozen() - 2).max(0);
        entity.base_mut().set_ticks_frozen(frozen);
    }
    remove_frost(entity);
    try_add_frost(entity, level);
    if entity.base().tick_count % 40 == 0
        && entity.base().get_ticks_frozen() >= entity.get_ticks_required_to_freeze()
        && entity.can_freeze()
    {
        util::hurt(level, entity, |sources| sources.freeze(), 1.0);
    }
    // push
    if l(entity).auto_spin_attack_ticks > 0 {
        lm(entity).auto_spin_attack_ticks -= 1;
        let now = entity.base().bounding_box();
        check_auto_spin_attack(entity, level, &before_travel_box, &now);
    }
    entity.push_entities(level);
    if entity.is_sensitive_to_water() && util::is_in_water_or_rain(entity, level) {
        util::hurt(level, entity, |sources| sources.drown(), 1.0);
    }
}
