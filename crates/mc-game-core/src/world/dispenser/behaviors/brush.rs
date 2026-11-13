//! `Items.BRUSH`: brushes a scute off an armadillo in front, else fails.

use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, play_optional_sound,
};
use crate::world::item::{ItemStack, hurt_and_break_stack};
use mc_core::phys::AABB;
use mc_data::EntityType;

struct BrushDispense(OptionalState);

impl DispenseTemplate for BrushDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let pos = source.pos.relative(source.facing());
        let level = &mut *source.level;
        let armadillos = level.get_entities_of_class(
            &AABB::from_block_pos(&pos),
            |e| e.get_type() == EntityType::ARMADILLO,
            |e| !e.is_spectator(),
        );
        for id in armadillos {
            let brushed =
                level.with_entity(id, |level, e| e.brush_off_scute(level, None, &dispensed));
            if brushed == Some(true) {
                hurt_and_break_stack(&mut dispensed, 16, level, None);
                return dispensed;
            }
        }
        self.0.set_success(false);
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(&["minecraft:brush"], |_| Box::new(Templated(
    BrushDispense(OptionalState::new())
)));
