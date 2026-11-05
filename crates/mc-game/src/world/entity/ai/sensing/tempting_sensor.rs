//! `TemptingSensor`: the closest player holding something this mob is tempted by.

use super::distance_to_sqr;
use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::TEMPTING_PLAYER;
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::entity::Entity;
use crate::world::item::ItemStack;
use mc_data::{Attribute, Item};

pub struct TemptingSensor {
    temptations: fn(&dyn Entity, &ItemStack) -> bool,
}

fn item_tag(tag: &str, stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

impl TemptingSensor {
    /// `new TemptingSensor(Predicate<ItemStack>)` for the frog / nautilus food tags.
    pub const fn new(temptations: fn(&dyn Entity, &ItemStack) -> bool) -> Self {
        Self { temptations }
    }

    /// `TemptingSensor.forAnimal()`: `m instanceof Animal animal ? animal.isFood(i) : false`.
    pub const fn for_animal() -> Self {
        Self::new(|mob, stack| mob.mob().is_some_and(|m| m.animal.is_some()) && mob.is_food(stack))
    }

    /// `FrogAi.getTemptations()`.
    pub const fn frog() -> Self {
        Self::new(|_, stack| item_tag("frog_food", stack))
    }

    /// `NautilusAi.getTemptations()`.
    pub const fn nautilus() -> Self {
        Self::new(|_, stack| item_tag("nautilus_food", stack))
    }

    fn player_holding_temptation(&self, mob: &dyn Entity, player: &dyn Entity) -> bool {
        (self.temptations)(mob, &player.get_item_by_slot(EquipmentSlot::MainHand)) || (self.temptations)(mob, &player.get_item_by_slot(EquipmentSlot::OffHand))
    }
}

impl Sensor for TemptingSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![TEMPTING_PLAYER.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let range = body.living().map_or(0.0, |l| l.get_attribute_value(Attribute::TEMPT_RANGE));
        let targeting = TargetingConditions::for_non_combat().ignore_line_of_sight().range(f64::from(range as f32));
        let body_ref: &dyn Entity = &*body;
        let mut players: Vec<i32> = level
            .players()
            .iter()
            .copied()
            .filter(|&id| {
                level.get_entity(id).is_some_and(|p| {
                    !p.is_spectator() && targeting.test(level, Some(body_ref), p) && self.player_holding_temptation(body_ref, p) && !body_ref.base().passengers.contains(&p.id())
                })
            })
            .collect();
        let distance = |id: &i32| level.get_entity(*id).map_or(f64::NAN, |p| distance_to_sqr(body_ref, p));
        players.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        body.brain_mut().set_memory_opt(&TEMPTING_PLAYER, players.first().copied().map(EntityRef));
    }
}
