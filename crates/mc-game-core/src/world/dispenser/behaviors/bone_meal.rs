//! `Items.BONE_MEAL`: grows the crop or water plant in front.

use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, play_optional_sound,
};
use crate::world::item::ItemStack;
use crate::world::item::bone_meal_item::{grow_crop, grow_water_plant};
use crate::world::level::LevelAccessor;

struct BoneMealDispense(OptionalState);

impl DispenseTemplate for BoneMealDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        self.0.set_success(true);
        let target = source.pos.relative(source.facing());
        let level = &mut *source.level;
        if !grow_crop(&mut dispensed, level, target)
            && !grow_water_plant(&mut dispensed, level, target, None)
        {
            self.0.set_success(false);
        } else {
            level.level_event(None, 1505, target, 15);
        }
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(&["minecraft:bone_meal"], |_| Box::new(Templated(
    BoneMealDispense(OptionalState::new())
)));
