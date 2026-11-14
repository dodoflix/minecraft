//! `Mob.tick/baseTick/aiStep/serverAiStep` and the additions of `AgeableMob`, `Animal`, `Monster` to them.

use super::{MobData, ext, navigation};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::Flag;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::living_entity::{
    self,
    util::{is_in_water_or_rain, l, lm},
};
use crate::world::entity::{Entity, EntityId};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, SoundEvent};

fn m(e: &dyn Entity) -> &MobData {
    e.mob().expect("mob")
}

fn mm(e: &mut dyn Entity) -> &mut MobData {
    e.mob_mut().expect("mob")
}

/// `Mob.tick()`.
pub fn tick(e: &mut dyn Entity, level: &mut ServerLevel) {
    // a size change made earlier in this tick (the pufferfish puff state) pushes the mob out of blocks before it moves
    crate::world::entity::movement::apply_pending_fudge(e, level);
    living_entity::tick::tick(e, level);
    if e.base().tick_count % 5 == 0 {
        e.update_control_flags(level);
    }
}

fn is_abstract_boat(ty: EntityType) -> bool {
    let name = ty.data().name;
    name.ends_with("_boat") || name.ends_with("_raft")
}

/// `updateControlFlags()`.
pub fn update_control_flags(e: &mut dyn Entity, level: &mut ServerLevel) {
    update_control_flags_with(e, level, |controller| controller.mob().is_some());
}

/// `updateControlFlags()` where `takes_control(controller)` says whether the controlling passenger takes over the goals' move,
/// jump and look flags (`Mob`: any mob; `Ravager`: a mob that is not a raider). Returns whether the goals keep control.
pub fn update_control_flags_with(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    takes_control: impl Fn(&dyn Entity) -> bool,
) -> bool {
    let controller_takes_over = e
        .get_controlling_passenger()
        .and_then(|id| level.get_entity(id))
        .is_some_and(takes_control);
    let in_boat = e
        .base()
        .vehicle
        .and_then(|v| level.get_entity(v))
        .is_some_and(|v| is_abstract_boat(v.get_type()));
    let no_controller = !controller_takes_over;
    let not_in_boat = !in_boat;
    let sel = &mut mm(e).goal_selector;
    sel.set_control_flag(Flag::Move, no_controller);
    sel.set_control_flag(Flag::Jump, no_controller && not_in_boat);
    sel.set_control_flag(Flag::Look, no_controller);
    no_controller
}

/// `Mob.baseTick()`.
pub fn base_tick(e: &mut dyn Entity, level: &mut ServerLevel) {
    living_entity::base_tick::base_tick(e, level);
    if e.is_alive() {
        let roll = e.base_mut().random.next_int_bounded(1000);
        let mob = mm(e);
        let time = mob.ambient_sound_time;
        mob.ambient_sound_time += 1;
        if roll < time {
            reset_ambient_sound_time(e);
            e.play_ambient_sound(level);
        }
    }
}

/// `resetAmbientSoundTime()`.
pub fn reset_ambient_sound_time(e: &mut dyn Entity) {
    let interval = e.get_ambient_sound_interval();
    mm(e).ambient_sound_time = -interval;
}

/// `playAmbientSound()`.
pub fn play_ambient_sound(e: &mut dyn Entity, level: &mut ServerLevel) {
    if let Some(sound) = e.get_ambient_sound_in(level) {
        living_entity::damage::make_sound(e, level, sound);
    }
}

/// `Mob.playHurtSound(source)`: resets the ambient timer, then the living hurt sound.
pub fn reset_ambient_before_hurt_sound(e: &mut dyn Entity) {
    reset_ambient_sound_time(e);
}

/// `Mob.tickHeadTurn(yBodyRotT)`: `bodyRotationControl.clientTick()`.
pub fn tick_head_turn(e: &mut dyn Entity) {
    if let Some(mut control) = mm(e).body_rotation_control.take() {
        control.client_tick(e);
        mm(e).body_rotation_control = Some(control);
    }
}

/// `Mob.serverAiStep()` (final in vanilla).
pub fn server_ai_step(e: &mut dyn Entity, level: &mut ServerLevel) {
    lm(e).no_action_time = l(e).no_action_time.wrapping_add(1);
    mm(e).sensing.tick();
    let id_based_tick_count = e.base().tick_count.wrapping_add(e.id());
    let (mut target_selector, mut goal_selector) = {
        let mob = mm(e);
        (
            std::mem::take(&mut mob.target_selector),
            std::mem::take(&mut mob.goal_selector),
        )
    };
    if id_based_tick_count % 2 != 0 && e.base().tick_count > 1 {
        target_selector.tick_running_goals(e, level, false);
        goal_selector.tick_running_goals(e, level, false);
    } else {
        target_selector.tick(e, level);
        goal_selector.tick(e, level);
    }
    {
        let mob = mm(e);
        mob.target_selector = target_selector;
        mob.goal_selector = goal_selector;
    }
    if let Some(mut nav) = mm(e).navigation.take() {
        nav.tick(e, level);
        if let Some(mob) = e.mob_mut() {
            mob.navigation = Some(nav);
        }
    }
    e.custom_server_ai_step(level);
    if let Some(mut c) = mm(e).move_control.take() {
        c.tick(e, level);
        mm(e).move_control = Some(c);
    }
    if let Some(mut c) = mm(e).look_control.take() {
        c.tick(e, level);
        mm(e).look_control = Some(c);
    }
    if let Some(mut c) = mm(e).jump_control.take() {
        c.tick(e, level);
        mm(e).jump_control = Some(c);
    }
}

/// `clampHeadRotationToBody()`.
pub fn clamp_head_rotation_to_body(e: &mut dyn Entity) {
    let limit = e.get_max_head_y_rot() as f32;
    let living = lm(e);
    let head_y_rot = living.y_head_rot;
    let delta = mth::wrap_degrees_f32(living.y_body_rot - head_y_rot);
    let target_delta = mth::clamp_f32(
        mth::wrap_degrees_f32(living.y_body_rot - head_y_rot),
        -limit,
        limit,
    );
    let new_head_y_rot = head_y_rot + delta - target_delta;
    e.set_y_head_rot(new_head_y_rot);
}

/// `Mob.aiStep()` plus the `AgeableMob` / `Animal` / `Monster` additions that apply to `e`: the `super.aiStep()` of every
/// concrete mob class.
pub fn ai_step(e: &mut dyn Entity, level: &mut ServerLevel) {
    if m(e).kind == super::MobKind::Monster {
        e.update_no_action_time(level);
    }
    mob_ai_step(e, level);
    if m(e).ageable.is_some() {
        super::ageable::ai_step_after(e, level);
    }
    if m(e).animal.is_some() {
        super::animal::ai_step_after(e, level);
    }
}

/// `Mob.aiStep()` alone.
pub fn mob_ai_step(e: &mut dyn Entity, level: &mut ServerLevel) {
    living_entity::ai_step::ai_step(e, level);
    if mc_data::tags::get::<EntityType>("burn_in_daylight")
        .is_some_and(|t| t.contains(e.get_type()))
    {
        burn_undead(e, level);
    }
    // looting
    if e.can_pick_up_loot() && e.is_alive() && !l(e).dead && level.mob_griefing() {
        let (rx, ry, rz) = e.get_pickup_reach();
        let bb = e
            .base()
            .bounding_box()
            .inflate(f64::from(rx), f64::from(ry), f64::from(rz));
        let items =
            level.get_entities_of_class(&bb, |x| x.get_type() == EntityType::ITEM, |_| true);
        for id in items {
            let wants = level
                .get_entity(id)
                .and_then(|x| x.as_any().downcast_ref::<ItemEntity>())
                .is_some_and(|item| {
                    let stack = item.get_item();
                    !item.is_removed()
                        && !stack.is_empty()
                        && !item.has_pick_up_delay()
                        && e.wants_to_pick_up(level, &stack)
                });
            if wants {
                e.pick_up_item(level, id);
            }
        }
    }
}

/// `sunProtectionSlot()`.
pub fn sun_protection_slot() -> EquipmentSlot {
    EquipmentSlot::Head
}

/// `burnUndead()`.
fn burn_undead(e: &mut dyn Entity, level: &mut ServerLevel) {
    if e.is_alive() && is_sun_burn_tick(e, level) {
        let slot = e.sun_protection_slot();
        let blocker = e.get_item_by_slot(slot);
        if !blocker.is_empty() {
            if blocker.is_damageable_item() {
                let amount = e.base_mut().random.next_int_bounded(2);
                e.hurt_and_break_equipment(level, slot, amount);
            }
        } else {
            e.ignite_for_seconds(8.0);
        }
    }
}

/// `isSunBurnTick()`.
pub fn is_sun_burn_tick(e: &mut dyn Entity, level: &ServerLevel) -> bool {
    if ext::monsters_burn_at(level, e.position()) {
        let br = e.get_light_level_dependent_magic_value(level);
        let rounded_pos = BlockPos::containing(e.base().x(), e.base().get_eye_y(), e.base().z());
        let in_non_burnable_block = is_in_water_or_rain(e, level)
            || e.base().is_in_powder_snow
            || e.base().was_in_powder_snow;
        if br > 0.5
            && e.base_mut().random.next_float() * 30.0 < (br - 0.4) * 2.0
            && !in_non_burnable_block
            && level_can_see_sky(level, rounded_pos)
        {
            return true;
        }
    }
    false
}

fn level_can_see_sky(level: &ServerLevel, pos: BlockPos) -> bool {
    use crate::world::level::LevelReader;
    level.can_see_sky(pos)
}

/// `Monster.updateNoActionTime()`.
pub fn monster_update_no_action_time(e: &mut dyn Entity, level: &ServerLevel) {
    let br = e.get_light_level_dependent_magic_value(level);
    if br > 0.5 {
        lm(e).no_action_time = l(e).no_action_time.wrapping_add(2);
    }
}

/// `Mob.stopInPlace()`.
pub fn stop_in_place(e: &mut dyn Entity, level: &mut ServerLevel) {
    navigation::stop(e, level);
    super::set_xxa(e, 0.0);
    super::set_yya(e, 0.0);
    e.set_speed(0.0);
    e.base_mut().set_delta_movement_xyz(0.0, 0.0, 0.0);
    if let Some(ld) = mm(e).leash_data.as_mut() {
        ld.angular_momentum = 0.0;
    }
}

/// `Mob.getMaxFallDistance()`.
pub fn get_max_fall_distance(e: &dyn Entity, level: &ServerLevel) -> i32 {
    let target = get_target_checked(e, level);
    if target.is_none() {
        return comfortable_fall_distance(0.0);
    }
    let living = l(e);
    let mut sacrifice = (living.get_health() - living.get_max_health() * 0.33f32) as i32;
    sacrifice -= (3 - level.difficulty().get_id()) * 4;
    if sacrifice < 0 {
        sacrifice = 0;
    }
    comfortable_fall_distance(sacrifice as f32)
}

/// `getComfortableFallDistance(allowedDamage)`.
pub fn comfortable_fall_distance(allowed_damage: f32) -> i32 {
    mth::floor_f32(allowed_damage + 3.0)
}

/// `getTarget()`: the target (`Entity::get_target`, which brain mobs read from the `ATTACK_TARGET` memory) if it is still a
/// valid one (`asValidTarget`).
pub fn get_target_checked(e: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    as_valid_target(e, level, e.get_target())
}

/// `asValidTarget(target)`.
pub fn as_valid_target(
    e: &dyn Entity,
    level: &ServerLevel,
    target: Option<EntityId>,
) -> Option<EntityId> {
    let id = target?;
    // vanilla holds the object; `canAttack` rejects a removed target except for a warden (`canTargetEntity`)
    let t = level.get_entity_or_recently_removed(id)?;
    if t.is_player() && (ext::is_creative_player(t) || t.is_spectator()) {
        return None;
    }
    if !e.can_attack(level, t) {
        return None;
    }
    Some(id)
}

/// `setTarget(target)`: stored when valid; a target that cannot be resolved right now (it is out of the level while it
/// runs) is stored as given.
pub fn set_target(e: &mut dyn Entity, level: &ServerLevel, target: Option<EntityId>) {
    let valid = match target {
        Some(id) if level.get_entity(id).is_some() => as_valid_target(e, level, target),
        other => other,
    };
    mm(e).target = valid;
}

/// `playSound` helper for the generic mob sounds.
pub fn play(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    sound: SoundEvent,
    volume: f32,
    pitch: f32,
) {
    e.play_sound(level, sound, volume, pitch);
}
