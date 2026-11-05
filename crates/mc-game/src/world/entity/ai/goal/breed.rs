//! `BreedGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::mob::{self, animal, ext, navigation};
use crate::world::entity::{Entity, EntityId};
use std::any::TypeId;
use std::sync::Arc;

/// The class of the partners: the animal's own concrete type, or an arbitrary `instanceof` test.
enum PartnerClass {
    Exact(TypeId),
    Test(Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>),
}

pub struct BreedGoal {
    partner_class: PartnerClass,
    partner: Option<EntityId>,
    love_time: i32,
    speed_modifier: f64,
    partner_targeting: TargetingConditions,
}

impl BreedGoal {
    /// `BreedGoal(animal, speedModifier)`.
    pub fn new(animal: &dyn Entity, speed_modifier: f64) -> Self {
        Self::with_class(speed_modifier, animal.as_any().type_id())
    }

    /// `BreedGoal(animal, speedModifier, clazz)`.
    pub fn with_class(speed_modifier: f64, partner_class: TypeId) -> Self {
        Self::build(speed_modifier, PartnerClass::Exact(partner_class))
    }

    /// `BreedGoal(animal, speedModifier, clazz)` for a class that spans several Rust types (`AbstractHorse`).
    pub fn with_class_test(speed_modifier: f64, partner_class: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>) -> Self {
        Self::build(speed_modifier, PartnerClass::Test(partner_class))
    }

    fn build(speed_modifier: f64, partner_class: PartnerClass) -> Self {
        Self {
            partner_class,
            partner: None,
            love_time: 0,
            speed_modifier,
            partner_targeting: TargetingConditions::for_non_combat().range(8.0).ignore_line_of_sight(),
        }
    }

    /// `getFreePartner()`.
    fn get_free_partner(&self, animal: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
        let class = &self.partner_class;
        let bb = animal.base().bounding_box().inflate_scalar(8.0);
        let animals = ext::get_nearby_entities(
            level,
            |e| match class {
                PartnerClass::Exact(type_id) => e.as_any().type_id() == *type_id,
                PartnerClass::Test(test) => test(e),
            },
            &self.partner_targeting,
            animal,
            &bb,
        );
        let mut dist = f64::MAX;
        let mut partner = None;
        for id in animals {
            let Some(potential) = level.get_entity(id) else { continue };
            if animal::can_mate(animal, potential) && !potential.is_panicking() && animal.base().distance_to_sqr_vec(potential.position()) < dist {
                partner = Some(id);
                dist = animal.base().distance_to_sqr_vec(potential.position());
            }
        }
        partner
    }
}

impl Goal for BreedGoal {
    fn can_use(&mut self, animal: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !animal::is_in_love(animal) {
            return false;
        }
        self.partner = self.get_free_partner(animal, level);
        self.partner.is_some()
    }

    fn can_continue_to_use(&mut self, _animal: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.partner.and_then(|p| level.get_entity(p)).is_some_and(|p| p.is_alive() && animal::is_in_love(p) && self.love_time < 60 && !p.is_panicking())
    }

    fn stop(&mut self, _animal: &mut dyn Entity, _level: &mut ServerLevel) {
        self.partner = None;
        self.love_time = 0;
    }

    fn tick(&mut self, animal: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(partner_id) = self.partner else { return };
        let max_head_x_rot = animal.get_max_head_x_rot() as f32;
        let Some(partner_pos) = level.get_entity(partner_id).map(|p| p.block_position()) else { return };
        if let Some(partner) = level.get_entity(partner_id) {
            mob::look_at_entity_full(animal, partner, 10.0, max_head_x_rot);
        }
        navigation::move_to_entity(animal, level, partner_pos, self.speed_modifier);
        self.love_time += 1;
        let close = level.get_entity(partner_id).is_some_and(|p| animal.base().distance_to_sqr_vec(p.position()) < 9.0);
        if self.love_time >= self.adjusted_tick_delay(60) && close {
            self.breed(animal, level, partner_id);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

impl BreedGoal {
    /// `breed()`.
    fn breed(&mut self, animal: &mut dyn Entity, level: &mut ServerLevel, partner: EntityId) {
        animal::spawn_child_from_breeding(animal, level, partner);
    }
}
