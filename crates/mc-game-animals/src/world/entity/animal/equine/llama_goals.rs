//! The goals nested in `Llama` / `TraderLlama`.

use super::llama::Llama;
use mc_data::EntityType;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetGoal, TargetType,
};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::targeting;
use mc_game_core::world::entity::mob::{combat, leashable, tamable};
use mc_game_core::world::entity::{Entity, EntityId};
use std::sync::Arc;

/// `Llama.LlamaHurtByTargetGoal`: gives up the target after spitting.
pub struct LlamaHurtByTargetGoal {
    inner: HurtByTargetGoal,
}

impl LlamaHurtByTargetGoal {
    pub fn new() -> Self {
        Self {
            inner: HurtByTargetGoal::new(Vec::new()),
        }
    }
}

impl Default for LlamaHurtByTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for LlamaHurtByTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if let Some(llama) = mob.as_any_mut().downcast_mut::<Llama>() {
            if llama.did_spit() {
                llama.set_did_spit(false);
                return false;
            }
        }
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Llama.LlamaAttackWolfGoal`: `NearestAttackableTargetGoal<Wolf>(llama, Wolf.class, 16, false, true, !tame)` with a quarter follow distance.
pub fn llama_attack_wolf_goal() -> NearestAttackableTargetGoal {
    let selector: targeting::TargetSelector =
        Arc::new(|target: &dyn Entity, _level: &ServerLevel| !tamable::is_tame(target));
    let mut goal = NearestAttackableTargetGoal::build(
        TargetType::of_type(EntityType::WOLF),
        16,
        false,
        true,
        Some(selector),
    );
    goal.target_goal.follow_distance_factor = 0.25;
    goal
}

/// `TraderLlama.TraderLlamaDefendWanderingTraderGoal`.
pub struct TraderLlamaDefendWanderingTraderGoal {
    target_goal: TargetGoal,
    owner_last_hurt_by: Option<EntityId>,
    timestamp: i32,
}

impl TraderLlamaDefendWanderingTraderGoal {
    pub fn new() -> Self {
        Self {
            target_goal: TargetGoal::new(false, false),
            owner_last_hurt_by: None,
            timestamp: 0,
        }
    }
}

impl Default for TraderLlamaDefendWanderingTraderGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for TraderLlamaDefendWanderingTraderGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !leashable::is_leashed(mob) {
            return false;
        }
        let Some(owner) = leashable::get_leash_holder(mob)
            .and_then(|h| level.get_entity(h))
            .filter(|h| h.wandering_trader_despawn_delay().is_some())
        else {
            return false;
        };
        let hurt_by = combat::get_last_hurt_by_mob(owner, level);
        let timestamp = combat::get_last_hurt_by_mob_timestamp(owner);
        self.owner_last_hurt_by = hurt_by;
        timestamp != self.timestamp
            && self
                .target_goal
                .can_attack(mob, level, hurt_by, &targeting::default_conditions())
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_target(level, self.owner_last_hurt_by);
        if let Some(owner) = leashable::get_leash_holder(mob)
            .and_then(|h| level.get_entity(h))
            .filter(|h| h.wandering_trader_despawn_delay().is_some())
        {
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
