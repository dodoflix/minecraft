//! `Blocks.TNT` (`OptionalDispenseItemBehavior` of `DispenseItemBehavior.bootStrap`).

use super::sulfur_cube::dispense_block;
use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, play_optional_sound,
};
use crate::world::entity::Entity;
use crate::world::entity::item::primed_tnt::PrimedTnt;
use crate::world::item::ItemStack;
use crate::world::level::gameevent::GameEventContext;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::SoundSource;

struct TntDispense(OptionalState);

impl DispenseTemplate for TntDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        if !source.level.tnt_explodes() {
            self.0.set_success(false);
            return dispensed;
        }
        let target = source.pos.relative(source.facing());
        if dispense_block(source.level, target, &mut dispensed) {
            return dispensed;
        }
        let tnt = PrimedTnt::at(
            source.level,
            f64::from(target.x()) + 0.5,
            f64::from(target.y()),
            f64::from(target.z()) + 0.5,
            None,
        );
        let at = tnt.position();
        source.level.add_fresh_entity(Box::new(tnt));
        source.level.play_sound_at(
            None,
            at.x,
            at.y,
            at.z,
            SoundEvent::ENTITY_TNT_PRIMED,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        source.level.game_event_at_block(
            GameEvent::ENTITY_PLACE,
            target,
            &GameEventContext::of_entity(None),
        );
        dispensed.shrink(1);
        self.0.set_success(true);
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(&["minecraft:tnt"], |_| Box::new(Templated(TntDispense(
    OptionalState::new()
))));
