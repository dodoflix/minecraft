//! `DefendVillageTargetGoal`: an iron golem targets a player a nearby villager hates (reputation <= -100).

use super::target_goal::TargetGoal;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::mob::ext;
use crate::world::entity::{Entity, EntityId};
use mc_data::EntityType;

pub struct DefendVillageTargetGoal {
    target_goal: TargetGoal,
    /// Never reset by vanilla (a stale value is re-checked by `canUse`).
    potential_target: Option<EntityId>,
    attack_targeting: TargetingConditions,
}

impl DefendVillageTargetGoal {
    /// `DefendVillageTargetGoal(golem)`: `super(golem, false, true)`.
    pub fn new() -> Self {
        Self { target_goal: TargetGoal::new(false, true), potential_target: None, attack_targeting: TargetingConditions::for_combat().range(64.0) }
    }
}

impl Goal for DefendVillageTargetGoal {
    fn can_use(&mut self, golem: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let grow = golem.base().bounding_box().inflate(10.0, 8.0, 10.0);
        let villagers = ext::get_nearby_entities(level, |e| e.get_type() == EntityType::VILLAGER, &self.attack_targeting, &*golem, &grow);
        let players: Vec<EntityId> = level
            .players()
            .iter()
            .copied()
            .filter(|id| {
                level.get_entity(*id).is_some_and(|p| {
                    let b = p.base();
                    grow.contains(b.x(), b.y(), b.z()) && self.attack_targeting.test(level, Some(&*golem), p)
                })
            })
            .collect();
        for villager in villagers {
            for player in &players {
                let Some(uuid) = level.get_entity(*player).map(|p| p.uuid()) else { continue };
                let reputation = level.get_entity(villager).map_or(0, |v| v.get_player_reputation(&uuid));
                if reputation <= -100 {
                    self.potential_target = Some(*player);
                }
            }
        }
        match self.potential_target.and_then(|t| level.get_entity(t)) {
            None => false,
            Some(t) => !(t.is_player() && (t.is_spectator() || ext::is_creative_player(t))),
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, golem: &mut dyn Entity, level: &mut ServerLevel) {
        golem.set_target(level, self.potential_target);
        self.target_goal.start();
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target_goal.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Target])
    }
}
