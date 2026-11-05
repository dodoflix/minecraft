//! `OwnerHurtTargetGoal`: a tamed animal attacks whatever its owner attacked.

use mc_game_core::world::entity::ai::goal::target::target_goal::TargetGoal;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::targeting::default_conditions;
use mc_game_core::world::entity::mob::tamable;
use mc_game_core::world::entity::{Entity, EntityId};

pub struct OwnerHurtTargetGoal {
    target_goal: TargetGoal,
    owner_last_hurt: Option<EntityId>,
    timestamp: i32,
}

impl OwnerHurtTargetGoal {
    pub fn new() -> Self {
        Self { target_goal: TargetGoal::new(false, false), owner_last_hurt: None, timestamp: 0 }
    }
}

impl Goal for OwnerHurtTargetGoal {
    fn can_use(&mut self, tame_animal: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !tamable::is_tame(tame_animal) || tamable::is_ordered_to_sit(tame_animal) {
            return false;
        }
        let Some(owner_id) = tamable::get_owner(tame_animal, level) else { return false };
        let Some(owner) = level.get_entity(owner_id) else { return false };
        self.owner_last_hurt = owner.living().and_then(|l| l.last_hurt_mob);
        let ts = owner.living().map_or(0, |l| l.last_hurt_mob_timestamp);
        ts != self.timestamp
            && self.target_goal.can_attack(tame_animal, level, self.owner_last_hurt, &default_conditions())
            && match (self.owner_last_hurt.and_then(|t| level.get_entity(t)), level.get_entity(owner_id)) {
                (Some(target), Some(owner)) => tame_animal.wants_to_attack(level, target, owner),
                _ => false,
            }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_target(level, self.owner_last_hurt);
        if let Some(owner) = tamable::get_owner(mob, level).and_then(|o| level.get_entity(o)) {
            self.timestamp = owner.living().map_or(0, |l| l.last_hurt_mob_timestamp);
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
