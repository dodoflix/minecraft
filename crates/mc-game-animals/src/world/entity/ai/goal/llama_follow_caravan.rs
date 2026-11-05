//! `LlamaFollowCaravanGoal`.

use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::EntityId;
use crate::world::entity::animal::equine::Llama;
use mc_game_core::world::entity::mob::{leashable, navigation};
use mc_core::phys::Vec3;
use mc_data::EntityType;

const CARAVAN_LIMIT: i32 = 8;

pub struct LlamaFollowCaravanGoal {
    speed_modifier: f64,
    dist_check_counter: i32,
}

impl LlamaFollowCaravanGoal {
    /// `LlamaFollowCaravanGoal(llama, speedModifier)`.
    pub fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier, dist_check_counter: 0 }
    }

    fn llama(mob: &dyn Entity) -> &Llama {
        mob.as_any().downcast_ref::<Llama>().expect("llama")
    }

    /// `firstIsLeashed(currentMob, counter)`; the first mob is the running llama itself (out of the level).
    fn first_is_leashed(level: &ServerLevel, current: &Llama, mut counter: i32) -> bool {
        if counter > CARAVAN_LIMIT {
            return false;
        }
        let Some(head) = current.get_caravan_head() else { return false };
        let Some(head_entity) = level.get_entity(head) else { return false };
        if leashable::is_leashed(head_entity) {
            true
        } else {
            counter += 1;
            match head_entity.as_any().downcast_ref::<Llama>() {
                Some(head_llama) => Self::first_is_leashed(level, head_llama, counter),
                None => false,
            }
        }
    }
}

impl Goal for LlamaFollowCaravanGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let llama = Self::llama(mob);
        if leashable::is_leashed(mob) || llama.in_caravan() {
            return false;
        }
        let bb = mob.base().bounding_box().inflate(9.0, 4.0, 9.0);
        let llamas = level.get_entities(Some(mob.id()), &bb, |e| e.get_type() == EntityType::LLAMA || e.get_type() == EntityType::TRADER_LLAMA);
        let mut closest: Option<EntityId> = None;
        let mut closest_dist_square = f64::MAX;
        for &id in &llamas {
            let Some(candidate) = level.get_entity(id) else { continue };
            let Some(c) = candidate.as_any().downcast_ref::<Llama>() else { continue };
            if c.in_caravan() && !c.has_caravan_tail() {
                let dist_square = mob.base().distance_to_sqr_vec(candidate.position());
                if !(dist_square > closest_dist_square) {
                    closest_dist_square = dist_square;
                    closest = Some(id);
                }
            }
        }
        if closest.is_none() {
            for &id in &llamas {
                let Some(candidate) = level.get_entity(id) else { continue };
                let Some(c) = candidate.as_any().downcast_ref::<Llama>() else { continue };
                if leashable::is_leashed(candidate) && !c.has_caravan_tail() {
                    let dist_square = mob.base().distance_to_sqr_vec(candidate.position());
                    if !(dist_square > closest_dist_square) {
                        closest_dist_square = dist_square;
                        closest = Some(id);
                    }
                }
            }
        }
        let Some(closest) = closest else { return false };
        if closest_dist_square < 4.0 {
            return false;
        }
        let closest_entity = level.get_entity(closest).expect("closest llama");
        let closest_llama = closest_entity.as_any().downcast_ref::<Llama>().expect("llama");
        if !leashable::is_leashed(closest_entity) && !Self::first_is_leashed(level, closest_llama, 1) {
            return false;
        }
        if let Some(llama) = mob.as_any_mut().downcast_mut::<Llama>() {
            llama.join_caravan(level, closest);
        }
        true
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let llama = Self::llama(mob);
        let Some(head) = llama.get_caravan_head() else { return false };
        let Some(head_entity) = level.get_entity(head) else { return false };
        if head_entity.is_alive() && Self::first_is_leashed(level, llama, 0) {
            let dist_sqr = mob.base().distance_to_sqr_vec(head_entity.position());
            if dist_sqr > 676.0 {
                if self.speed_modifier <= 3.0 {
                    self.speed_modifier *= 1.2;
                    self.dist_check_counter = reduced_tick_delay(40);
                    return true;
                }
                if self.dist_check_counter == 0 {
                    return false;
                }
            }
            if self.dist_check_counter > 0 {
                self.dist_check_counter -= 1;
            }
            true
        } else {
            false
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(llama) = mob.as_any_mut().downcast_mut::<Llama>() {
            llama.leave_caravan(level);
        }
        self.speed_modifier = 2.1;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let llama = Self::llama(mob);
        if !llama.in_caravan() {
            return;
        }
        let holder_is_knot = leashable::get_leash_holder(mob).and_then(|h| level.get_entity(h)).is_some_and(|h| h.get_type() == EntityType::LEASH_KNOT);
        if holder_is_knot {
            return;
        }
        let Some(follows) = llama.get_caravan_head().and_then(|h| level.get_entity(h)) else { return };
        let follows_pos = follows.base().position();
        let distance_to = mob.base().distance_to_sqr_vec(follows_pos).sqrt() as f32;
        let delta = Vec3::new(follows_pos.x - mob.base().x(), follows_pos.y - mob.base().y(), follows_pos.z - mob.base().z()).normalize().scale((f64::from(distance_to) - 2.0).max(0.0));
        let (x, y, z) = (mob.base().x() + delta.x, mob.base().y() + delta.y, mob.base().z() + delta.z);
        navigation::move_to(mob, level, x, y, z, self.speed_modifier);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
