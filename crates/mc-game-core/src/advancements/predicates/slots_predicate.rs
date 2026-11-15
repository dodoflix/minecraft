//! `net.minecraft.advancements.predicates.SlotsPredicate`.

use crate::commands::arguments::slot_arguments::{SlotRange, SlotRanges};
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::Entity;
use mc_dfu::{Codec, string_resolver};
use mc_protocol::components::ItemPredicate;
use std::sync::LazyLock;

/// `Map<SlotRange, ItemPredicate>` in input order.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotsPredicate {
    pub slots: Vec<(SlotRange, ItemPredicate)>,
}

impl SlotsPredicate {
    pub fn codec() -> Codec<SlotsPredicate> {
        static C: LazyLock<Codec<SlotsPredicate>> = LazyLock::new(|| {
            let slot_range = string_resolver(
                |r: &SlotRange| Some(r.get_serialized_name().to_owned()),
                |name| SlotRanges::name_to_ids(name).cloned(),
            );
            slot_range.unbounded_map(&ItemPredicate::codec()).xmap(
                |slots| SlotsPredicate { slots },
                |p: &SlotsPredicate| p.slots.clone(),
            )
        });
        C.clone()
    }

    /// `matches(SlotProvider)`: for every range some slot holds a matching stack. Reading `container.N` of a chest vehicle
    /// (`getChestVehicleSlot(slot).get()`) unpacks its pending loot table first.
    pub fn matches(&self, entity: &dyn Entity, level: &ServerLevel) -> bool {
        self.slots.iter().all(|(range, predicate)| {
            range.slots().iter().any(|&slot| {
                if let Some(chest) = entity.chest_vehicle()
                    && usize::try_from(slot).is_ok_and(|s| s < chest.get_container_size())
                {
                    chest.unpack_shared(level, entity.position());
                }
                entity
                    .get_slot(slot)
                    .is_some_and(|stack| predicate.test(&stack))
            })
        })
    }
}
