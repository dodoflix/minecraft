//! `PiercingWeapon`: the stab of a spear (`ServerboundPlayerActionPacket.Action.STAB`): the entities along the view line, the
//! `canHitEntity` filter, and the `LivingEntity`/`Player` `stabAttack` both the stab and the lunge of a kinetic weapon use.

use super::InteractionHand;
use super::consumable::holder_sound;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::combat;
use crate::world::entity::living_entity::damage::as_server_player;
use crate::world::entity::player::attack::get_attack_range_with;
use crate::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
use crate::world::entity::projectile::util::get_hit_entities_along;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::collision::ClipBlock;
use mc_data::{Attribute, EntityType};
use mc_protocol::components::values::combat::PiercingWeapon;
use mc_protocol::components::{ATTACK_ANIMATION, Holder, SoundEvent as ComponentSoundEvent};

/// `PiercingWeapon.canHitEntity(jabber, target)`.
pub fn can_hit_entity(level: &ServerLevel, jabber: &dyn Entity, target: &dyn Entity) -> bool {
    if target.is_invulnerable_to_piercing_weapon() || !target.is_alive() {
        return false;
    }
    if target.get_type() == EntityType::INTERACTION {
        return true;
    }
    if !target.can_be_hit_by_projectile() {
        return false;
    }
    if let (Some(jabbing_player), true) = (
        jabber.as_any().downcast_ref::<ServerPlayer>(),
        target.is_player(),
    ) && !jabbing_player.can_harm_player(level, &target.get_scoreboard_name())
    {
        return false;
    }
    !jabber.is_passenger_of_same_vehicle(level, target)
}

/// `LivingEntity.stabAttack(...)` or, for a player, `Player.stabAttack(...)`; `target` is taken out of the level.
pub fn stab_attack(
    level: &mut ServerLevel,
    attacker: &mut dyn Entity,
    slot: EquipmentSlot,
    target: &mut dyn Entity,
    base_damage: f32,
    deals_damage: bool,
    deals_knockback: bool,
    dismounts: bool,
) -> bool {
    match as_server_player(&mut *attacker) {
        Some(player) => player.stab_attack(
            level,
            slot,
            target,
            base_damage,
            deals_damage,
            deals_knockback,
            dismounts,
        ),
        None => combat::stab_attack(
            level,
            attacker,
            slot,
            target,
            base_damage,
            deals_damage,
            deals_knockback,
            dismounts,
        ),
    }
}

/// `level.playSound(except ? player : null, x, y, z, sound, player.getSoundSource(), 1, 1)` for a sound of the component.
fn play_sound(
    level: &mut ServerLevel,
    player: &ServerPlayer,
    sound: Option<&Holder<ComponentSoundEvent>>,
    except: bool,
) {
    if let Some(sound) = sound.and_then(holder_sound) {
        let p = player.position();
        level.play_sound_at(
            except.then(|| player.id()),
            p.x,
            p.y,
            p.z,
            sound,
            player.get_sound_source(),
            1.0,
            1.0,
        );
    }
}

/// `PiercingWeapon.attack(attacker, hand)` for a player (the only caller is the STAB action).
pub fn attack(
    piercing: &PiercingWeapon,
    level: &mut ServerLevel,
    attacker: &mut ServerPlayer,
    slot: EquipmentSlot,
) {
    let damage = attacker
        .player
        .living
        .get_attribute_value(Attribute::ATTACK_DAMAGE) as f32;
    let weapon_item = attacker.get_item_by_slot(slot);
    let swing_animation = weapon_item
        .get(&ATTACK_ANIMATION)
        .copied()
        .unwrap_or(SWING_ANIMATION_DEFAULT);
    let attack_range = get_attack_range_with(&*attacker, &weapon_item);
    let hits = {
        let jabber: &dyn Entity = &*attacker;
        let level: &ServerLevel = level;
        get_hit_entities_along(
            level,
            jabber,
            &attack_range,
            &|target| can_hit_entity(level, jabber, target),
            ClipBlock::Collider,
        )
        .unwrap_or_default()
    };
    let mut hit_something = false;
    for hit in hits {
        hit_something |= level
            .with_entity(hit.entity, |level, target| {
                stab_attack(
                    level,
                    attacker,
                    slot,
                    target,
                    damage,
                    true,
                    piercing.deals_knockback,
                    piercing.dismounts,
                )
            })
            .unwrap_or(false);
    }
    Entity::on_attack(attacker);
    combat::post_piercing_attack(attacker, level);
    if hit_something {
        play_sound(level, attacker, piercing.hit_sound.as_ref(), false);
    }
    play_sound(level, attacker, piercing.sound.as_ref(), true);
    if attacker.swing(level, InteractionHand::MainHand, swing_animation, false) {
        attacker.reset_attack_strength_ticker();
    }
}

/// The dismount of a stab: `target.isPassenger() && !target.is(EntityTypeTags.CANNOT_BE_DISMOUNTED_BY_ITEM_USAGE)` then
/// `target.stopRiding()`; `true` when the target was dismounted.
pub fn dismount(level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
    let immune = mc_data::tags::get::<EntityType>("cannot_be_dismounted_by_item_usage")
        .is_some_and(|t| t.contains(target.get_type()));
    if target.base().is_passenger() && !immune {
        level.stop_riding(target);
        return true;
    }
    false
}
