//! `net.minecraft.world.item.MaceItem`: the smash attack (fall-based bonus damage, `mace_smash` damage source, smash sounds,
//! area knockback, fall-damage immunity after the hit). The weapon's attributes/tool data come from the item's components.

use super::{ItemBehaviour, ItemStack};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::decoration::armor_stand::ArmorStand;
use crate::world::entity::living_entity::damage::as_server_player;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment_helper;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::{Attribute, SoundEvent};
use mc_protocol::codec::LpVec3;
use mc_protocol::packets::play::ClientboundSetEntityMotionPacket;

pub struct MaceItem;

/// `MaceItem.canSmashAttack(attacker)`.
pub fn can_smash_attack(attacker: &dyn Entity) -> bool {
    attacker.base().fall_distance > 1.5 && !attacker.living().is_some_and(|l| l.is_fall_flying())
}

/// `MaceItem.calculateImpactPosition(attacker)`.
fn calculate_impact_position(attacker: &dyn Entity) -> Vec3 {
    match attacker.living().and_then(|l| l.current_impulse_impact_pos) {
        Some(pos) if pos.y <= attacker.position().y => pos,
        _ => attacker.position(),
    }
}

/// `LivingEntity.setIgnoreFallDamageFromCurrentImpulse(true, impactPos)`.
fn set_ignore_fall_damage_from_current_impulse(attacker: &mut dyn Entity, impact_pos: Vec3) {
    let living = attacker.living_mut().expect("living");
    living.current_impulse_context_reset_grace_time =
        living.current_impulse_context_reset_grace_time.max(40);
    living.current_impulse_impact_pos = Some(impact_pos);
}

fn send_motion(entity: &mut dyn Entity) {
    let id = entity.id();
    let movement = LpVec3(entity.base().delta_movement());
    if let Some(player) = as_server_player(entity) {
        player.send(&ClientboundSetEntityMotionPacket { id, movement });
    }
}

/// `MaceItem.getKnockbackPower(attacker, nearby, direction)`.
fn knockback_power(attacker: &dyn Entity, nearby: &dyn Entity, direction: Vec3) -> f64 {
    let resistance = nearby.living().map_or(0.0, |l| {
        l.get_attribute_value(Attribute::KNOCKBACK_RESISTANCE)
    });
    (3.5 - direction.length())
        * f64::from(0.7f32)
        * if attacker.base().fall_distance > 5.0 {
            2.0
        } else {
            1.0
        }
        * (1.0 - resistance)
}

/// `MaceItem.knockback(level, attacker, entity)`: the smash pushes the living entities around the target.
fn knockback(level: &mut ServerLevel, attacker: &mut dyn Entity, entity: &mut dyn Entity) {
    let on_pos: BlockPos = crate::world::entity::movement::get_on_pos(&*entity, &*level, 1.0E-5);
    level.level_event_impl(None, 2013, on_pos, 750);
    let area = entity.get_bounding_box().inflate(3.5, 3.5, 3.5);
    let (attacker_id, target_id) = (attacker.id(), entity.id());
    let nearby_ids: Vec<EntityId> = level.get_entities(None, &area, |e| e.living().is_some());
    for id in nearby_ids {
        let Some(nearby) = level.get_entity(id) else {
            continue;
        };
        // `knockbackPredicate`; not implemented: a `TamableAnimal` owned by the attacker is not excluded
        let not_spectator = !nearby.is_spectator();
        let not_player = id != attacker_id && id != target_id;
        let not_allied =
            !crate::world::effect::mob_effect_util::is_allied_to(level, &*attacker, nearby);
        let not_armor_stand = !nearby
            .as_any()
            .downcast_ref::<ArmorStand>()
            .is_some_and(|a| a.is_marker());
        let within_range = entity.base().distance_to_sqr_vec(nearby.position()) <= 12.25;
        let not_flying_in_creative = !nearby
            .player()
            .is_some_and(|p| p.abilities.instabuild && p.abilities.flying);
        if !(not_spectator
            && not_player
            && not_allied
            && not_armor_stand
            && within_range
            && not_flying_in_creative)
        {
            continue;
        }
        let direction = nearby.position().subtract(entity.position());
        let power = knockback_power(&*attacker, nearby, direction);
        let vector = direction.normalize().scale(power);
        if power > 0.0 {
            level.with_entity(id, |_, nearby| {
                nearby.push_xyz(vector.x, f64::from(0.7f32), vector.z);
                send_motion(nearby);
            });
        }
    }
}

impl ItemBehaviour for MaceItem {
    fn hurt_enemy(
        &self,
        level: &mut ServerLevel,
        _stack: &mut ItemStack,
        mob: &mut dyn Entity,
        attacker: &mut dyn Entity,
    ) {
        if !can_smash_attack(attacker) {
            return;
        }
        let m = attacker.base().delta_movement();
        attacker
            .base_mut()
            .set_delta_movement(Vec3::new(m.x, f64::from(0.01f32), m.z));
        let impact = calculate_impact_position(attacker);
        set_ignore_fall_damage_from_current_impulse(attacker, impact);
        send_motion(attacker);
        let sound = if mob.base().on_ground() {
            if let Some(player) = as_server_player(attacker) {
                player.spawn_extra_particles_on_fall = true;
            }
            if attacker.base().fall_distance > 5.0 {
                SoundEvent::ITEM_MACE_SMASH_GROUND_HEAVY
            } else {
                SoundEvent::ITEM_MACE_SMASH_GROUND
            }
        } else {
            SoundEvent::ITEM_MACE_SMASH_AIR
        };
        let p = attacker.position();
        level.play_sound_at(
            None,
            p.x,
            p.y,
            p.z,
            sound,
            attacker.get_sound_source(),
            1.0,
            1.0,
        );
        knockback(level, attacker, mob);
    }

    fn post_hurt_enemy(
        &self,
        _level: &mut ServerLevel,
        _stack: &mut ItemStack,
        _mob: &mut dyn Entity,
        attacker: &mut dyn Entity,
    ) {
        if can_smash_attack(attacker) {
            attacker.reset_fall_distance();
        }
    }

    fn get_attack_damage_bonus_for(
        &self,
        level: &mut ServerLevel,
        _stack: &ItemStack,
        victim: &mut dyn Entity,
        _damage: f32,
        source: &DamageSource,
        attacker: &mut dyn Entity,
    ) -> f32 {
        // `damageSource.getDirectEntity() instanceof LivingEntity attacker`
        if source.get_direct_entity() != Some(attacker.id())
            || attacker.living().is_none()
            || !can_smash_attack(attacker)
        {
            return 0.0;
        }
        let fall_distance = attacker.base().fall_distance;
        let damage = if fall_distance <= 3.0 {
            4.0 * fall_distance
        } else if fall_distance <= 8.0 {
            12.0 + 2.0 * (fall_distance - 3.0)
        } else {
            22.0 + fall_distance - 8.0
        };
        let weapon = attacker.get_weapon_item().unwrap_or_else(ItemStack::empty);
        let victim_id = victim.id();
        let fall_based = enchantment_helper::modify_fall_based_damage(
            level,
            &mut Actors::of(attacker).and(victim),
            &weapon,
            victim_id,
            source,
            0.0,
        );
        (damage + f64::from(fall_based) * fall_distance) as f32
    }

    fn get_item_damage_source(
        &self,
        level: &ServerLevel,
        attacker: &dyn Entity,
    ) -> Option<DamageSource> {
        can_smash_attack(attacker).then(|| level.damage_sources().mace(attacker))
    }
}

register_item_class!("MaceItem", |_item| Box::new(MaceItem));
