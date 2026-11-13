//! `Mob.convertTo`, `ConversionParams`, `ConversionType`.

use super::{ageable, ext};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::living_entity::util::{l, lm};
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

/// `ConversionType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConversionType {
    Single,
    SplitOnDeath,
}

impl ConversionType {
    /// `shouldDiscardAfterConversion()`.
    pub fn should_discard_after_conversion(self) -> bool {
        self == ConversionType::Single
    }
}

/// `ConversionParams` (`team` = the scoreboard team name).
#[derive(Clone, Debug)]
pub struct ConversionParams {
    pub ty: ConversionType,
    pub keep_equipment: bool,
    pub preserve_can_pick_up_loot: bool,
    pub team: Option<String>,
}

impl ConversionParams {
    /// `ConversionParams.single(mob, keepEquipment, preserveCanPickUpLoot)`.
    pub fn single(
        mob: &dyn Entity,
        level: &ServerLevel,
        keep_equipment: bool,
        preserve_can_pick_up_loot: bool,
    ) -> Self {
        Self {
            ty: ConversionType::Single,
            keep_equipment,
            preserve_can_pick_up_loot,
            team: team_of(level, mob),
        }
    }
}

/// `Entity.getTeam()` as the team name.
fn team_of(level: &ServerLevel, e: &dyn Entity) -> Option<String> {
    let scoreboard = level.scoreboard();
    scoreboard
        .get_players_team(&e.get_scoreboard_name())
        .map(|t| t.get_name().to_string())
}

/// `Mob.convertTo(type, params, spawnReason, afterConversion)`: the converted mob's id (already added to the level).
/// `after_conversion` is `AfterConversion.finalizeConversion` (runs before the mob is added).
pub fn convert_to(
    from: &mut dyn Entity,
    level: &mut ServerLevel,
    ty: EntityType,
    params: &ConversionParams,
    spawn_reason: EntitySpawnReason,
    after_conversion: impl FnOnce(&mut dyn Entity, &mut ServerLevel),
) -> Option<crate::world::entity::EntityId> {
    if from.is_removed() {
        return None;
    }
    let mut new_mob = factory::create(
        ty,
        level,
        EntitySpawnRequest {
            reason: spawn_reason,
            ignore_checks: false,
        },
    )?;
    // Not implemented: converting into an entity type that has no class yet (the witch a struck villager becomes): the
    // inert stand-in cannot take the converted mob's state, so the conversion is skipped.
    new_mob.living()?;
    convert(from, new_mob.as_mut(), level, params);
    after_conversion(new_mob.as_mut(), level);
    let new_id = new_mob.id();
    level.add_fresh_entity(new_mob);
    if params.ty == ConversionType::Single {
        relink_riding(from, level, new_id);
    }
    if params.ty.should_discard_after_conversion() {
        from.discard_in_level(level);
    }
    Some(new_id)
}

/// The riding part of `ConversionType.SINGLE.convert` (needs the converted mob in the level).
fn relink_riding(
    from: &mut dyn Entity,
    level: &mut ServerLevel,
    to_id: crate::world::entity::EntityId,
) {
    let root_passenger = from.base().passengers.first().copied();
    if let Some(root) = root_passenger {
        // the rider leaves the converting mob, which is out of its slot while its own code runs
        level.lend(from, |level| {
            level.with_entity(root, |level, p| {
                level.stop_riding(p);
                p.base_mut().boarding_cooldown = 0;
            });
        });
        let existing: Vec<_> = level
            .get_entity(to_id)
            .map(|t| t.base().passengers.clone())
            .unwrap_or_default();
        for passenger in existing {
            level.with_entity(passenger, |level, p| {
                level.stop_riding(p);
                level.remove_held_entity(p, crate::world::entity::RemovalReason::Discarded);
            });
        }
        level.with_entity(root, |level, p| {
            level.start_riding(p, to_id, false, true);
        });
    }
    if let Some(vehicle) = from.base().vehicle {
        level.stop_riding(from);
        level.with_entity(to_id, |level, t| {
            level.start_riding(t, vehicle, false, false);
        });
    }
}

/// `ConversionType.convert(from, to, params)` without the riding links (see [`relink_riding`]).
fn convert(
    from: &mut dyn Entity,
    to: &mut dyn Entity,
    level: &mut ServerLevel,
    params: &ConversionParams,
) {
    match params.ty {
        ConversionType::Single => {
            let (p, yr, xr) = (from.position(), from.base().y_rot(), from.base().x_rot());
            to.snap_to(p.x, p.y, p.z, yr, xr);
            to.base_mut()
                .set_delta_movement(from.base().delta_movement());
            if params.keep_equipment {
                for slot in EquipmentSlot::VALUES {
                    let item = from.get_item_by_slot(slot);
                    if !item.is_empty() {
                        lm(from).equipment.get_mut(slot).set_count(0);
                        super::equipment::set_item_slot_spawn(to, slot, item);
                        let chance = from
                            .mob()
                            .map_or(0.085, |m| m.drop_chances.by_equipment(slot));
                        super::equipment::set_drop_chance(to, slot, chance);
                    }
                }
            }
            to.base_mut().fall_distance = from.base().fall_distance;
            let fall_flying = l(from).is_fall_flying();
            to.base_mut()
                .set_shared_flag(crate::world::entity::entity::FLAG_FALL_FLYING, fall_flying);
            lm(to).last_hurt_by_player_memory_time = l(from).last_hurt_by_player_memory_time;
            lm(to).hurt_time = l(from).hurt_time;
            lm(to).y_body_rot = l(from).y_body_rot;
            let on_ground = from.base().on_ground();
            to.base_mut().set_on_ground_raw(on_ground);
            if let Some(pos) = l(from).get_sleeping_pos() {
                lm(to).set_sleeping_pos(pos);
            }
            if let Some(holder) = super::leashable::get_leash_holder(from) {
                let info = level
                    .get_entity(holder)
                    .map(super::leashable::LeashHolder::of);
                if let Some(info) = info {
                    super::leashable::set_leashed_to(to, level, info, true);
                }
            }
            convert_common(from, to, level, params);
        }
        ConversionType::SplitOnDeath => {
            if let Some(root) = from.base().passengers.first().copied() {
                level.lend(from, |level| {
                    level.with_entity(root, |level, p| level.stop_riding(p));
                });
            }
            if super::leashable::get_leash_holder(from).is_some() {
                super::leashable::drop_leash(from, level);
            }
            convert_common(from, to, level, params);
        }
    }
}

/// `ConversionType.convertCommon(from, to, params)`.
fn convert_common(
    from: &mut dyn Entity,
    to: &mut dyn Entity,
    level: &mut ServerLevel,
    params: &ConversionParams,
) {
    let absorption = from.get_absorption_amount();
    to.set_absorption_amount(absorption);
    let effects: Vec<MobEffectInstance> = l(from).active_effects.values().cloned().collect();
    for effect in effects {
        to.add_effect(level, effect, None);
    }
    if from.is_baby() {
        to.set_baby(true);
    }
    if from.mob().is_some_and(|m| m.ageable.is_some())
        && to.mob().is_some_and(|m| m.ageable.is_some())
    {
        let (age, forced_age, forced_age_timer) = {
            let a = from
                .mob()
                .and_then(|m| m.ageable.as_ref())
                .expect("ageable");
            (a.age, a.forced_age, a.forced_age_timer)
        };
        ageable::set_age(to, age);
        if let Some(a) = to.mob_mut().and_then(|m| m.ageable.as_mut()) {
            a.forced_age = forced_age;
            a.forced_age_timer = forced_age_timer;
        }
    }
    ext::copy_angry_at_memory(from, to);
    if params.preserve_can_pick_up_loot {
        let v = from.can_pick_up_loot();
        to.set_can_pick_up_loot(v);
    }
    let (left_handed, no_ai, persistent) = {
        let m = from.mob().expect("mob");
        (m.is_left_handed(), m.is_no_ai(), m.persistence_required)
    };
    {
        let t = to.mob_mut().expect("mob");
        t.set_left_handed(left_handed);
        t.set_no_ai(no_ai);
        if persistent {
            t.persistence_required = true;
        }
    }
    let name_visible = from.base().is_custom_name_visible();
    to.base_mut().set_custom_name_visible(name_visible);
    let on_fire = from.is_on_fire();
    to.set_shared_flag_on_fire(on_fire);
    to.base_mut().permanently_invulnerable = from.base().permanently_invulnerable;
    to.base_mut().invulnerable_time = from.base().invulnerable_time;
    let no_gravity = from.base().is_no_gravity();
    to.base_mut().set_no_gravity(no_gravity);
    to.base_mut().portal_cooldown = from.base().portal_cooldown;
    let silent = from.base().is_silent();
    to.base_mut().set_silent(silent);
    let tags: Vec<String> = from
        .base()
        .tags
        .iter_java_order()
        .into_iter()
        .map(|t| t.to_string())
        .collect();
    for t in tags {
        to.base_mut().add_tag(&t);
    }
    // COMPONENTS_TO_COPY: custom name and custom data
    let name = from.base().get_custom_name();
    if name.is_some() {
        to.set_custom_name(name);
    }
    if !from.base().custom_data.is_empty() {
        to.base_mut().custom_data = from.base().custom_data.clone();
    }
    if let Some(team) = &params.team {
        let from_team = team_of(level, from);
        let to_name = to.get_scoreboard_name();
        let from_name = from.get_scoreboard_name();
        let mut scoreboard = level.scoreboard_mut();
        scoreboard.add_player_to_team(&to_name, team);
        if from_team.as_deref() == Some(team.as_str()) {
            let _ = scoreboard.remove_player_from_team_of(&from_name, team);
        }
    }
    from.conversion_common_extra(to);
}
