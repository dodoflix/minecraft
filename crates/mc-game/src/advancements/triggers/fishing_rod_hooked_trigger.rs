//! `FishingRodHookedTrigger`: `fishing_rod_hooked`.

use super::prelude::*;

/// `FishingRodHookedTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct FishingRodHookedTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub rod: Option<ItemPredicate>,
    pub entity: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl TriggerInstance for FishingRodHookedTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<FishingRodHookedTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(FishingRodHookedTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                rod: ItemPredicate::codec().optional_field_of("rod"),
                entity: condition_holder_codec().optional_field_of("entity"),
                item: ItemPredicate::codec().optional_field_of("item"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for FishingRodHookedTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(FishingRodHookedTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "entity", &this.entity);
});

crate::register_triggers!(FishingRodHookedTriggerInstance {
    FISHING_ROD_HOOKED = "fishing_rod_hooked",
});

impl FishingRodHookedTriggerInstance {
    /// `matches(rod, hookedIn, items)`; `hooked_in_item` is the stack when the hooked-in entity is an item entity.
    pub fn matches(&self, rod: &ItemStack, hooked_in: &mut LootContext, hooked_in_item: Option<&ItemStack>, items: &[ItemStack]) -> bool {
        if self.rod.as_ref().is_some_and(|p| !p.test(rod)) {
            return false;
        }
        if self.entity.is_some() && !holder_matches(&self.entity, hooked_in) {
            return false;
        }
        if let Some(item) = &self.item {
            let matched = hooked_in_item.is_some_and(|s| item.test(s)) || items.iter().any(|s| item.test(s));
            if !matched {
                return false;
            }
        }
        true
    }
}

impl Trigger<FishingRodHookedTriggerInstance> {
    /// `trigger(player, rod, hook, items)`: `hooked_in` is `hook.getHookedIn()` or, when nothing is hooked, the hook itself.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, rod: &ItemStack, hooked_in: &dyn Entity, items: &[ItemStack]) {
        let matches = self.matches(&*level, &*player, rod, hooked_in, items);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, rod: &ItemStack, hooked_in: &dyn Entity, items: &[ItemStack]) -> TriggerMatches {
        let params = entity_params(level, player.position(), hooked_in);
        let mut context = new_context(&params);
        let hooked_in_item = hooked_in.as_any().downcast_ref::<crate::world::entity::item::ItemEntity>().map(|e| e.get_item().clone());
        self.collect(level, player, |t| t.matches(rod, &mut context, hooked_in_item.as_ref(), items))
    }
}
