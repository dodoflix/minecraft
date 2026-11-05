//! `Mob`/`LivingEntity` combat plumbing the generic combat goals use: `getLastHurtByMob`, `isWithinMeleeAttackRange`,
//! `getAttackBoundingBox`, `swing`/`swingForAttack`, `getActiveItem`, `doHurtTarget` dispatch.

use crate::server::level::ServerLevel;
use crate::server::level::level_events::encode_packet;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living::LIVING_ENTITY_FLAG_IS_USING;
use crate::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::{InteractionHand, ItemStack};
use mc_core::phys::AABB;
use mc_data::MobEffect;
use mc_protocol::components::{ATTACK_ANIMATION, ATTACK_RANGE};
use mc_protocol::packets::play::{ClientboundSwingAnimationPacket, SwingAnimation};

/// `Mob.DEFAULT_ATTACK_REACH = Math.sqrt(2.04F) - 0.6F`.
pub fn default_attack_reach() -> f64 {
    f64::from(2.04_f32).sqrt() - f64::from(0.6_f32)
}

/// `getLastHurtByMob()` (`EntityReference.getLivingEntity`).
pub fn get_last_hurt_by_mob(e: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    let uuid = e.living()?.last_hurt_by_mob?;
    level.get_entity_by_uuid(uuid).filter(|t| t.living().is_some()).map(|t| t.id())
}

/// `getLastHurtByMobTimestamp()`.
pub fn get_last_hurt_by_mob_timestamp(e: &dyn Entity) -> i32 {
    e.living().map_or(0, |l| l.last_hurt_by_mob_timestamp)
}

/// `getActiveItem()`: the item in use, else the main hand item (nothing for spectators).
pub fn get_active_item(e: &dyn Entity) -> ItemStack {
    if e.is_spectator() {
        return ItemStack::empty();
    }
    let living = e.living().expect("living");
    if living.get_living_entity_flag(LIVING_ENTITY_FLAG_IS_USING) { living.use_item.clone() } else { e.get_item_by_slot(EquipmentSlot::MainHand) }
}

/// `getAttackBoundingBox(horizontalExpansion)`.
pub fn get_attack_bounding_box(e: &dyn Entity, level: &ServerLevel, horizontal_expansion: f64) -> AABB {
    let own = e.get_bounding_box();
    let aabb = match e.base().vehicle.and_then(|v| level.get_entity(v)) {
        Some(vehicle) => {
            let mount = vehicle.get_bounding_box();
            AABB::new(own.min_x.min(mount.min_x), own.min_y, own.min_z.min(mount.min_z), own.max_x.max(mount.max_x), own.max_y, own.max_z.max(mount.max_z))
        }
        None => own,
    };
    aabb.inflate(horizontal_expansion, 0.0, horizontal_expansion)
}

/// `isWithinMeleeAttackRange(target)`.
pub fn is_within_melee_attack_range(e: &dyn Entity, level: &ServerLevel, target: &dyn Entity) -> bool {
    let active = get_active_item(e);
    let (max_range, min_range) = match active.get(&ATTACK_RANGE) {
        None => (default_attack_reach(), 0.0),
        // `effectiveMaxRange(this)` / `effectiveMinRange(this)`: `this` is a mob here, never a player
        Some(range) => (f64::from(range.max_reach * range.mob_factor), f64::from(range.min_reach * range.mob_factor)),
    };
    let hitbox = target.get_bounding_box();
    get_attack_bounding_box(e, level, max_range).intersects_aabb(&hitbox) && (min_range <= 0.0 || !get_attack_bounding_box(e, level, min_range).intersects_aabb(&hitbox))
}

/// `getModifiedSwingDuration(animation)`.
fn get_modified_swing_duration(e: &dyn Entity, animation: &SwingAnimation) -> i32 {
    let dig = [MobEffect::HASTE, MobEffect::CONDUIT_POWER].iter().filter_map(|m| e.get_effect(*m).map(|i| i.amplifier)).max();
    match dig {
        Some(amplifier) => animation.duration - (1 + amplifier),
        None => match e.get_effect(MobEffect::MINING_FATIGUE) {
            Some(f) => animation.duration + (1 + f.amplifier) * 2,
            None => animation.duration,
        },
    }
}

/// `swing(hand, animation, sendToSwingingEntity)` of a non-player living entity.
pub fn swing_with(e: &mut dyn Entity, level: &mut ServerLevel, hand: InteractionHand, animation: SwingAnimation, send_to_swinging_entity: bool) -> bool {
    let duration = get_modified_swing_duration(e, &animation);
    if !e.living_mut().expect("living").swing_state.start_if_able(hand, animation, duration) {
        return false;
    }
    let packet = ClientboundSwingAnimationPacket { entity_id: e.id(), hand: hand.to_protocol(), animation };
    let payload = encode_packet(&packet);
    if send_to_swinging_entity {
        level.entity_tracker.send_to_tracking_players_and_self(e.id(), &payload);
    } else {
        level.entity_tracker.send_to_tracking_players(e.id(), &payload);
    }
    true
}

/// `Mob.swing(hand, animation)`.
pub fn swing(e: &mut dyn Entity, level: &mut ServerLevel, hand: InteractionHand, animation: SwingAnimation) {
    swing_with(e, level, hand, animation, false);
}

/// `Mob.swingForAttack(hand)`.
pub fn swing_for_attack(e: &mut dyn Entity, level: &mut ServerLevel, hand: InteractionHand) {
    let animation = e.get_item_by_slot(hand.as_equipment_slot()).get(&ATTACK_ANIMATION).copied().unwrap_or(SWING_ANIMATION_DEFAULT);
    swing(e, level, hand, animation);
}

/// `mob.doHurtTarget(level, target)`: the target is taken out of the level while the attack runs.
pub fn do_hurt_target(e: &mut dyn Entity, level: &mut ServerLevel, target: EntityId) -> bool {
    level.with_entity(target, |level, t| e.do_hurt_target(level, t)).unwrap_or(false)
}

/// `Entity.getTeam()` as the team's name (`Team` identity): the team of the scoreboard member `e` plays for (a tamed pet's
/// root owner's, like [`ext::is_allied_to`]).
pub fn get_team_name(level: &ServerLevel, e: &dyn Entity) -> Option<String> {
    let member = super::ext::team_member_name(level, e)?;
    level.scoreboard().get_players_team(&member).map(|t| t.get_name().to_string())
}

/// `Mob.lookAt(entity, yMax, xMax)`: turns the mob's own rotation towards `entity` (not the look control).
pub fn look_at(e: &mut dyn Entity, entity: &dyn Entity, y_max: f32, x_max: f32) {
    use mc_core::math::mth;
    let xd = entity.base().x() - e.base().x();
    let zd = entity.base().z() - e.base().z();
    let yd = if entity.living().is_some() {
        entity.base().get_eye_y() - e.base().get_eye_y()
    } else {
        let bb = entity.get_bounding_box();
        (bb.min_y + bb.max_y) / 2.0 - e.base().get_eye_y()
    };
    let sd = (xd * xd + zd * zd).sqrt();
    let y_rot_d = (mth::atan2(zd, xd) * f64::from(180.0_f32) / f64::from(std::f32::consts::PI)) as f32 - 90.0;
    let x_rot_d = (-(mth::atan2(yd, sd) * f64::from(180.0_f32) / f64::from(std::f32::consts::PI))) as f32;
    let new_x = rotlerp(e.base().x_rot(), x_rot_d, x_max);
    e.base_mut().set_x_rot(new_x);
    let new_y = rotlerp(e.base().y_rot(), y_rot_d, y_max);
    e.base_mut().set_y_rot(new_y);
}

/// `Mob.rotlerp(a, b, max)`.
fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut diff = mc_core::math::mth::wrap_degrees_f32(b - a);
    if diff > max {
        diff = max;
    }
    if diff < -max {
        diff = -max;
    }
    a + diff
}

/// `isHolding(item)`: main or off hand.
pub fn is_holding(e: &dyn Entity, item: mc_data::Item) -> bool {
    e.get_item_by_slot(EquipmentSlot::MainHand).item() == item || e.get_item_by_slot(EquipmentSlot::OffHand).item() == item
}
/// `getRootVehicle() instanceof Mob vehicleMob ? vehicleMob.chargeSpeedModifier() : 1.0F` (the root vehicle of an unmounted
/// mob is the mob itself).
pub fn root_charge_speed_modifier(mob: &dyn Entity, level: &ServerLevel) -> f32 {
    let root = mob.get_root_vehicle(level);
    if root == mob.id() {
        mob.charge_speed_modifier()
    } else {
        level.get_entity(root).filter(|e| e.mob().is_some()).map_or(1.0, |e| e.charge_speed_modifier())
    }
}
