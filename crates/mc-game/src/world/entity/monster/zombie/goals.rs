//! The goal registration of the zombie family (`addBehaviourGoals()` of `Zombie`, `Drowned`, `ZombifiedPiglin`).

use super::drowned::{DrownedGoToWaterGoal, DrownedSwimUpGoal};
use super::{Zombie, ZombieKind};
use crate::world::entity::ai::goal::{Goal, RandomStrollGoal, WaterAvoidingRandomStrollGoal};

/// `this instanceof BreakDoorGoal`.
pub fn is_break_door_goal(_goal: &dyn Goal) -> bool {
    false
}

/// `addBehaviourGoals()`.
pub fn add_behaviour_goals(zombie: &mut Zombie) {
    match zombie.kind {
        ZombieKind::Drowned(_) => {
            zombie.mob.goal_selector.add_goal(1, Box::new(DrownedGoToWaterGoal::new(1.0)));
            zombie.mob.goal_selector.add_goal(6, Box::new(DrownedSwimUpGoal::new(1.0)));
            zombie.mob.goal_selector.add_goal(7, Box::new(RandomStrollGoal::new(1.0)));
        }
        ZombieKind::ZombifiedPiglin(_) => {
            zombie.mob.goal_selector.add_goal(7, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        }
        _ => {
            zombie.mob.goal_selector.add_goal(7, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        }
    }
}
