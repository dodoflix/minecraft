//! `PickedUpItemTrigger`: `thrown_item_picked_up_by_entity`, `thrown_item_picked_up_by_player`.

use super::prelude::*;

/// `PickedUpItemTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct PickedUpItemTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
    pub entity: Option<ConditionHolder>,
}

impl TriggerInstance for PickedUpItemTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<PickedUpItemTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(PickedUpItemTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    item: ItemPredicate::codec().optional_field_of("item"),
                    entity: condition_holder_codec().optional_field_of("entity"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for PickedUpItemTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(PickedUpItemTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "entity",
        &this.entity,
    );
});

crate::register_triggers!(PickedUpItemTriggerInstance {
    THROWN_ITEM_PICKED_UP_BY_ENTITY = "thrown_item_picked_up_by_entity",
    THROWN_ITEM_PICKED_UP_BY_PLAYER = "thrown_item_picked_up_by_player",
});

impl PickedUpItemTriggerInstance {
    /// `matches(itemStack, entityContext)`.
    pub fn matches(&self, item_stack: &ItemStack, entity: &mut LootContext) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item_stack))
            && holder_matches(&self.entity, entity)
    }
}

impl Trigger<PickedUpItemTriggerInstance> {
    /// `trigger(player, itemStack, entity)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        item_stack: &ItemStack,
        entity: &dyn Entity,
    ) {
        let matches = self.matches(&*level, &*player, item_stack, entity);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        item_stack: &ItemStack,
        entity: &dyn Entity,
    ) -> TriggerMatches {
        let params = entity_params(level, player.position(), entity);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(item_stack, &mut context))
    }
}
