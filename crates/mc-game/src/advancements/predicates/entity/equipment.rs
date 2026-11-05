//! `EntityEquipmentPredicate`.

use super::entity_predicate::EntitySubPredicateImpl;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;
use mc_dfu::Codec;
use mc_protocol::components::ItemPredicate;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct EntityEquipmentPredicate {
    pub head: Option<ItemPredicate>,
    pub chest: Option<ItemPredicate>,
    pub legs: Option<ItemPredicate>,
    pub feet: Option<ItemPredicate>,
    pub body: Option<ItemPredicate>,
    pub mainhand: Option<ItemPredicate>,
    pub offhand: Option<ItemPredicate>,
}

impl EntityEquipmentPredicate {
    /// `matches(@Nullable Entity)`: only living entities have equipment.
    pub fn matches_entity(&self, entity: &dyn Entity) -> bool {
        if entity.living().is_none() {
            return false;
        }
        [
            (&self.head, EquipmentSlot::Head),
            (&self.chest, EquipmentSlot::Chest),
            (&self.legs, EquipmentSlot::Legs),
            (&self.feet, EquipmentSlot::Feet),
            (&self.body, EquipmentSlot::Body),
            (&self.mainhand, EquipmentSlot::MainHand),
            (&self.offhand, EquipmentSlot::OffHand),
        ]
        .into_iter()
        .all(|(predicate, slot)| predicate.as_ref().is_none_or(|p| p.test(&entity.get_item_by_slot(slot))))
    }
}

impl EntitySubPredicateImpl for EntityEquipmentPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<EntityEquipmentPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(EntityEquipmentPredicate {
                head: ItemPredicate::codec().optional_field_of("head"),
                chest: ItemPredicate::codec().optional_field_of("chest"),
                legs: ItemPredicate::codec().optional_field_of("legs"),
                feet: ItemPredicate::codec().optional_field_of("feet"),
                body: ItemPredicate::codec().optional_field_of("body"),
                mainhand: ItemPredicate::codec().optional_field_of("mainhand"),
                offhand: ItemPredicate::codec().optional_field_of("offhand"),
            })
            .codec()
        });
        C.clone()
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.matches_entity(entity)
    }
}
