//! `UseItemGoal<T extends Mob>`.

use mc_core::random::RandomSource;
use mc_data::SoundEvent;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::living_entity::using_item as use_item;
use mc_game_core::world::entity::mob::equipment;
use mc_game_core::world::item::{InteractionHand, ItemStack};
use std::sync::Arc;

/// `Predicate<? super T> canUseSelector`.
pub type CanUseSelector = Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

pub struct UseItemGoal {
    item: ItemStack,
    can_use_selector: CanUseSelector,
    finish_using_sound: Option<SoundEvent>,
}

impl UseItemGoal {
    /// `UseItemGoal(mob, item, finishUsingSound, canUseSelector)`.
    pub fn new(
        item: ItemStack,
        finish_using_sound: Option<SoundEvent>,
        can_use_selector: CanUseSelector,
    ) -> Self {
        Self {
            item,
            can_use_selector,
            finish_using_sound,
        }
    }
}

impl Goal for UseItemGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        (self.can_use_selector)(mob)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        use_item::is_using_item(mob)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        equipment::set_item_slot(mob, level, EquipmentSlot::MainHand, self.item.copy());
        use_item::start_using_item(mob, level, InteractionHand::MainHand);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        equipment::set_item_slot(mob, level, EquipmentSlot::MainHand, ItemStack::empty());
        if let Some(sound) = self.finish_using_sound {
            let pitch = mob.base_mut().random.next_float() * 0.2 + 0.9;
            mob.play_sound(level, sound, 1.0, pitch);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
