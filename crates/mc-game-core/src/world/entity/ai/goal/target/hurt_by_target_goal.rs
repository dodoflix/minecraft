//! `HurtByTargetGoal`.

use super::EntityClass;
use super::target_goal::TargetGoal;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::mob::{combat, ext, tamable, tick};
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::AABB;
use mc_data::EntityType;

pub const ALERT_RANGE_Y: f64 = 10.0;

/// `alertOther(other, hurtByMob)` override (default: `other.setTarget(hurtByMob)`), e.g. `PolarBear`: only non-babies.
pub type AlertOtherFn = Box<dyn Fn(&mut ServerLevel, EntityId, EntityId) + Send + Sync>;

pub struct HurtByTargetGoal {
    pub target_goal: TargetGoal,
    hurt_by_targeting: TargetingConditions,
    alert_same_type: bool,
    timestamp: i32,
    to_ignore_damage: Vec<EntityClass>,
    to_ignore_alert: Option<Vec<EntityClass>>,
    /// `getEntitiesOfClass(mob.getClass(), ..)` of `alertOthers`: `instanceof` test for the mob's class. Defaults to the
    /// mob's concrete Rust type; set it for classes with Java subclasses that are separate Rust types (`Zombie`).
    pub alert_class: Option<EntityClass>,
    /// `alertOther` override.
    pub alert_other: Option<AlertOtherFn>,
}

impl HurtByTargetGoal {
    /// `HurtByTargetGoal(mob, ignoreDamageFromTheseTypes...)`: `ignore_damage_from` are `isAssignableFrom` tests.
    pub fn new(ignore_damage_from: Vec<EntityClass>) -> Self {
        Self {
            target_goal: TargetGoal::new(true, false),
            hurt_by_targeting: TargetingConditions::for_combat()
                .ignore_line_of_sight()
                .ignore_invisibility_testing(),
            alert_same_type: false,
            timestamp: 0,
            to_ignore_damage: ignore_damage_from,
            to_ignore_alert: None,
            alert_class: None,
            alert_other: None,
        }
    }

    /// `setAlertOthers(exceptTheseTypes...)`: `except_these` compare `other.getClass() == ignoreClass`.
    pub fn set_alert_others(mut self, except_these: Vec<EntityClass>) -> Self {
        self.alert_same_type = true;
        self.to_ignore_alert = Some(except_these);
        self
    }

    /// `alertOthers()`.
    pub fn alert_others(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let within = self.target_goal.get_follow_distance(mob);
        let search_aabb: AABB = AABB::unit_cube_from_lower_corner(mob.position()).inflate(
            within,
            ALERT_RANGE_Y,
            within,
        );
        let mob_type_id = mob.as_any().type_id();
        let nearby = level.get_entities_of_class(
            &search_aabb,
            |e| {
                e.mob().is_some()
                    && self
                        .alert_class
                        .as_ref()
                        .map_or_else(|| e.as_any().type_id() == mob_type_id, |class| class(e))
            },
            |e| !e.is_spectator(),
        );
        let hurt_by = combat::get_last_hurt_by_mob(mob, level);
        let tamable_owner = mob
            .mob()
            .is_some_and(|m| m.tamable.is_some())
            .then(|| tamable::get_owner(mob, level));
        for other_id in nearby {
            let Some(other) = level.get_entity(other_id) else {
                continue;
            };
            if mob.id() == other.id() || tick::get_target_checked(other, level).is_some() {
                continue;
            }
            if let Some(owner) = tamable_owner {
                if owner != tamable::get_owner(other, level) {
                    continue;
                }
            }
            if hurt_by
                .and_then(|h| level.get_entity(h))
                .is_some_and(|h| ext::is_allied_to(level, other, h))
            {
                continue;
            }
            if let Some(ignore) = &self.to_ignore_alert {
                if ignore.iter().any(|class| class(other)) {
                    continue;
                }
            }
            self.alert_other(level, other_id, hurt_by);
        }
    }

    /// `alertOther(other, hurtByMob)`.
    fn alert_other(&self, level: &mut ServerLevel, other: EntityId, hurt_by: Option<EntityId>) {
        match (&self.alert_other, hurt_by) {
            (Some(f), Some(hurt_by)) => f(level, other, hurt_by),
            _ => {
                level.with_entity(other, |level, o| o.set_target(level, hurt_by));
            }
        }
    }
}

impl Goal for HurtByTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let timestamp = combat::get_last_hurt_by_mob_timestamp(mob);
        let last_hurt_by_mob = combat::get_last_hurt_by_mob(mob, level);
        if timestamp != self.timestamp {
            if let Some(hurt_by) = last_hurt_by_mob {
                let Some(hurt_by_entity) = level.get_entity(hurt_by) else {
                    return false;
                };
                if hurt_by_entity.get_type() == EntityType::PLAYER && level.universal_anger() {
                    return false;
                }
                if self
                    .to_ignore_damage
                    .iter()
                    .any(|class| class(hurt_by_entity))
                {
                    return false;
                }
                let conditions = self.hurt_by_targeting.clone();
                return self
                    .target_goal
                    .can_attack(mob, level, Some(hurt_by), &conditions);
            }
        }
        false
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let hurt_by = combat::get_last_hurt_by_mob(mob, level);
        mob.set_target(level, hurt_by);
        self.target_goal.target_mob = tick::get_target_checked(mob, level);
        self.timestamp = combat::get_last_hurt_by_mob_timestamp(mob);
        self.target_goal.unseen_memory_ticks = 300;
        if self.alert_same_type {
            self.alert_others(mob, level);
        }
        self.target_goal.start();
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target_goal.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Target])
    }
}
