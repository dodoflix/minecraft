//! `OwnerHurtByTargetGoal`: a tamed animal attacks whoever hurt its owner.

use mc_game_core::world::entity::ai::goal::target::target_goal::TargetGoal;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageTypeTags;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::targeting::default_conditions;
use mc_game_core::world::entity::mob::{combat, ext, tamable};
use mc_game_core::world::entity::{Entity, EntityId};

pub struct OwnerHurtByTargetGoal {
    target_goal: TargetGoal,
    owner_last_hurt_by: Option<EntityId>,
    timestamp: i32,
}

impl OwnerHurtByTargetGoal {
    pub fn new() -> Self {
        Self { target_goal: TargetGoal::new(false, false), owner_last_hurt_by: None, timestamp: 0 }
    }
}

impl Goal for OwnerHurtByTargetGoal {
    fn can_use(&mut self, tame_animal: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !tamable::is_tame(tame_animal) || tamable::is_ordered_to_sit(tame_animal) {
            return false;
        }
        let Some(owner_id) = tamable::get_owner(tame_animal, level) else { return false };
        let Some(owner) = level.get_entity(owner_id) else { return false };
        let last_damage_source = ext::get_last_damage_source(level, owner, 100);
        if !last_damage_source.is_some_and(|s| !s.is_tag(level, DamageTypeTags::NO_WOLF_RETALIATION)) {
            return false;
        }
        self.owner_last_hurt_by = combat::get_last_hurt_by_mob(owner, level);
        let ts = combat::get_last_hurt_by_mob_timestamp(owner);
        ts != self.timestamp
            && self.target_goal.can_attack(tame_animal, level, self.owner_last_hurt_by, &default_conditions())
            && match (self.owner_last_hurt_by.and_then(|t| level.get_entity(t)), level.get_entity(owner_id)) {
                (Some(target), Some(owner)) => tame_animal.wants_to_attack(level, target, owner),
                _ => false,
            }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_target(level, self.owner_last_hurt_by);
        if let Some(owner) = tamable::get_owner(mob, level).and_then(|o| level.get_entity(o)) {
            self.timestamp = combat::get_last_hurt_by_mob_timestamp(owner);
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
