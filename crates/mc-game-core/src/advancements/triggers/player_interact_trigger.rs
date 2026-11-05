//! `PlayerInteractTrigger`: `player_interacted_with_entity`, `player_sheared_equipment`.

use super::prelude::*;

/// `PlayerInteractTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct PlayerInteractTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
    pub entity: Option<ConditionHolder>,
}

impl TriggerInstance for PlayerInteractTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<PlayerInteractTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(PlayerInteractTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                item: ItemPredicate::codec().optional_field_of("item"),
                entity: condition_holder_codec().optional_field_of("entity"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for PlayerInteractTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(PlayerInteractTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "entity", &this.entity);
});

crate::register_triggers!(PlayerInteractTriggerInstance {
    PLAYER_INTERACTED_WITH_ENTITY = "player_interacted_with_entity",
    PLAYER_SHEARED_EQUIPMENT = "player_sheared_equipment",
});

impl PlayerInteractTriggerInstance {
    /// `matches(itemStack, entityContext)`.
    pub fn matches(&self, item_stack: &ItemStack, entity: &mut LootContext) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item_stack)) && holder_matches(&self.entity, entity)
    }
}

impl Trigger<PlayerInteractTriggerInstance> {
    /// `trigger(player, itemStack, interactedWith)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, item_stack: &ItemStack, entity: &dyn Entity) {
        let matches = self.matches(&*level, &*player, item_stack, entity);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, item_stack: &ItemStack, entity: &dyn Entity) -> TriggerMatches {
        let params = entity_params(level, player.position(), entity);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(item_stack, &mut context))
    }
}
