//! `Items.CHEST` (anonymous `OptionalDispenseItemBehavior` of `DispenseItemBehavior.bootStrap`): equips the chest onto a
//! tamed chest-less `AbstractChestedHorse` in front.

use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, default_execute, play_optional_sound,
};
use crate::world::item::ItemStack;
use mc_core::phys::AABB;

struct ChestDispense(OptionalState);

impl DispenseTemplate for ChestDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let pos = source.pos.relative(source.facing());
        let horses = source.level.get_entities_of_class(
            &AABB::from_block_pos(&pos),
            |e| e.chested_horse_has_chest() == Some(false),
            |e| e.is_alive(),
        );
        for id in horses {
            let equipped = source.level.with_entity(id, |_, horse| {
                horse.as_abstract_horse().is_some_and(|h| h.is_tamed())
                    && horse.chested_horse_set_chest_slot(&dispensed)
            });
            if equipped == Some(true) {
                dispensed.shrink(1);
                self.0.set_success(true);
                return dispensed;
            }
        }
        default_execute(source, dispensed)
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(&["minecraft:chest"], |_| Box::new(Templated(
    ChestDispense(OptionalState::new())
)));
