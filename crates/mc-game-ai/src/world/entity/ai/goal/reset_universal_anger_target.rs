//! `ResetUniversalAngerTargetGoal` (`T extends Mob & NeutralMob`).

use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::mob::{combat, neutral};
use mc_game_core::world::entity::Entity;
use mc_core::phys::AABB;
use mc_data::{Attribute, EntityType};

pub const ALERT_RANGE_Y: f64 = 10.0;

pub struct ResetUniversalAngerTargetGoal {
    alert_others_of_same_type: bool,
    last_hurt_by_player_timestamp: i32,
}

impl ResetUniversalAngerTargetGoal {
    pub fn new(alert_others_of_same_type: bool) -> Self {
        Self { alert_others_of_same_type, last_hurt_by_player_timestamp: 0 }
    }

    fn was_hurt_by_player(&self, mob: &dyn Entity, level: &ServerLevel) -> bool {
        let Some(hurt_by) = combat::get_last_hurt_by_mob(mob, level).and_then(|id| level.get_entity(id)) else { return false };
        hurt_by.get_type() == EntityType::PLAYER && mob.living().is_some_and(|l| l.last_hurt_by_mob_timestamp > self.last_hurt_by_player_timestamp)
    }
}

impl Goal for ResetUniversalAngerTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        level.universal_anger() && self.was_hurt_by_player(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.last_hurt_by_player_timestamp = mob.living().map_or(0, |l| l.last_hurt_by_mob_timestamp);
        neutral::forget_current_target_and_refresh_universal_anger(mob, level);
        if self.alert_others_of_same_type {
            let within = mob.living().map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE));
            let search_aabb = AABB::unit_cube_from_lower_corner(mob.position()).inflate(within, ALERT_RANGE_Y, within);
            let class = mob.as_any().type_id();
            let others = level.get_entities_of_class(&search_aabb, |e| e.as_any().type_id() == class, |e| !e.is_spectator());
            for other in others {
                level.with_entity(other, |level, e| neutral::forget_current_target_and_refresh_universal_anger(e, level));
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
