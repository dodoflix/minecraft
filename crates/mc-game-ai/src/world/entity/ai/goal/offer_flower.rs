//! `OfferFlowerGoal`.

use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_core::phys::AABB;
use mc_core::random::RandomSource;
use mc_data::{EntityType, Item};

pub const OFFER_TICKS: i32 = 400;

/// `IronGolem.offerFlower(offering)`: supplied by the golem's constructor (`|golem, offering| ...` downcasting to `IronGolem`).
pub type OfferFlowerFn = fn(&mut dyn Entity, &mut ServerLevel, bool);

pub struct OfferFlowerGoal {
    offer_target_context: TargetingConditions,
    entity: Option<EntityId>,
    tick: i32,
    offer_flower: OfferFlowerFn,
}

impl OfferFlowerGoal {
    pub fn new(offer_flower: OfferFlowerFn) -> Self {
        Self { offer_target_context: TargetingConditions::for_non_combat().range(6.0), entity: None, tick: 0, offer_flower }
    }

    fn get_golem_bounding_box(golem: &dyn Entity) -> AABB {
        golem.base().bounding_box().inflate(6.0, 2.0, 6.0)
    }
}

impl Goal for OfferFlowerGoal {
    fn can_use(&mut self, golem: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !level.is_bright_outside() {
            return false;
        }
        if golem.base_mut().random.next_int_bounded(8000) != 0 {
            return false;
        }
        let tag = mc_data::tags::get::<EntityType>("candidate_for_iron_golem_gift");
        let bb = Self::get_golem_bounding_box(golem);
        let candidates = level.get_entities_of_class(&bb, |e| e.living().is_some() && tag.is_some_and(|t| t.contains(e.get_type())), |_| true);
        let pos = golem.position();
        let mut best_distance = f64::MAX;
        self.entity = None;
        for id in candidates {
            let Some(e) = level.get_entity(id) else { continue };
            if self.offer_target_context.test(level, Some(golem), e) {
                let distance = e.base().distance_to_sqr_xyz(pos.x, pos.y, pos.z);
                if distance < best_distance {
                    best_distance = distance;
                    self.entity = Some(id);
                }
            }
        }
        self.entity.is_some()
    }

    fn can_continue_to_use(&mut self, _golem: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.tick > 0
    }

    fn start(&mut self, golem: &mut dyn Entity, level: &mut ServerLevel) {
        self.tick = self.adjusted_tick_delay(400);
        (self.offer_flower)(golem, level, true);
    }

    fn stop(&mut self, golem: &mut dyn Entity, level: &mut ServerLevel) {
        (self.offer_flower)(golem, level, false);
        if self.tick == 0 {
            if let Some(id) = self.entity {
                let accepts = mc_data::tags::get::<EntityType>("accepts_iron_golem_gift");
                let golem_bb = Self::get_golem_bounding_box(golem);
                let ok = level.get_entity(id).is_some_and(|m| {
                    m.mob().is_some()
                        && accepts.is_some_and(|t| t.contains(m.get_type()))
                        && m.get_item_by_slot(EquipmentSlot::Saddle).is_empty()
                        && golem_bb.intersects_aabb(&m.base().bounding_box())
                });
                if ok {
                    level.with_entity(id, |level, m| {
                        mob::equipment::set_item_slot(m, level, EquipmentSlot::Saddle, ItemStack::new(Item::POPPY));
                        mob::equipment::set_guaranteed_drop(m, EquipmentSlot::Saddle);
                    });
                }
            }
        }
        self.entity = None;
    }

    fn tick(&mut self, golem: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(e) = self.entity.and_then(|e| level.get_entity(e)) {
            mob::look_at_entity_full(golem, e, 30.0, 30.0);
        }
        self.tick -= 1;
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
