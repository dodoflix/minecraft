//! `Items.POTION`: a water bottle turns the block in front into mud when it is convertible, else it is dropped.

use crate::world::dispenser::{BlockSource, DispenseItemBehavior, DispenseTemplate, Templated, consume_with_remainder, default_behavior};
use crate::world::effect::potion_contents;
use crate::world::item::ItemStack;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{GameEvent, Item, ParticleType, Potion, SoundEvent};
use mc_protocol::components::POTION_CONTENTS;
use mc_protocol::packets::play::{ParticleOptions, SoundSource};

struct PotionDispense;

impl DispenseTemplate for PotionDispense {
    fn execute(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        let is_water = dispensed.get(&POTION_CONTENTS).is_some_and(|c| potion_contents::is(c, Potion::WATER));
        if !is_water {
            return default_behavior().dispense(source, dispensed);
        }
        let pos = source.pos;
        let target = pos.relative(source.facing());
        if !source.level.get_block_state(target).block().is_in_tag("convertible_to_mud") {
            return default_behavior().dispense(source, dispensed);
        }
        let level = &mut *source.level;
        for _ in 0..5 {
            let x = f64::from(pos.x()) + level.random().next_double();
            let z = f64::from(pos.z()) + level.random().next_double();
            level.send_particles(ParticleOptions::Simple(ParticleType::SPLASH), x, f64::from(pos.y() + 1), z, 1, 0.0, 0.0, 0.0, 1.0);
        }
        level.play_sound(None, pos, SoundEvent::ITEM_BOTTLE_EMPTY, SoundSource::Blocks, 1.0, 1.0);
        level.game_event_at_block(GameEvent::FLUID_PLACE, pos, &GameEventContext::of_entity(None));
        level.set_block_and_update(target, Block::MUD.default_state());
        consume_with_remainder(source, dispensed, ItemStack::new(Item::GLASS_BOTTLE))
    }
}

crate::register_dispense_behavior!(&["minecraft:potion"], |_| Box::new(Templated(PotionDispense)));
