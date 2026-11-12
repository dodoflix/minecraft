//! `Instrument` (the `instrument` registry) and `InstrumentItem` (goat horns).

use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStack, holder_sound, hurt_and_break,
    with_item_registries,
};
use crate::register_item_class;
use crate::registry::codec::{Lookup, RegistryDecodeContext};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::{Entity, ServerPlayer};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_data::GameEvent;
use mc_dfu::JSON;
use mc_protocol::components::common::serde_to_dyn;
use mc_protocol::components::{Holder, INSTRUMENT, Instrument, ItemUseAnimation};
use mc_protocol::packets::play::SoundSource;
use mc_protocol::registry_context::{ScopedLookup, with_scoped_lookup};
use serde_json::Value;

/// The `RegistryOps` lookup of the loading stage: entries of the registry being loaded reference each other.
pub struct StageLookup<'a, 'b>(pub &'a RegistryDecodeContext<'b>);

impl ScopedLookup for StageLookup<'_, '_> {
    fn contains(&self, registry: &str, name: &Identifier) -> Option<bool> {
        match self.0.lookup.contains(registry, name) {
            Lookup::NoRegistry => None,
            _ => Some(self.0.check_holder(registry, name).is_ok()),
        }
    }

    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Option<bool> {
        match self.0.lookup.tag_exists(registry, tag) {
            Lookup::NoRegistry => None,
            _ => Some(self.0.check_tag(registry, tag).is_ok()),
        }
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let input = serde_to_dyn(json);
    let instrument = with_scoped_lookup(&StageLookup(ctx), || {
        Instrument::direct_codec().parse(&JSON, &input)
    })
    .into_result()?;
    Ok(DecodedEntry::new(instrument))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::INSTRUMENT,
    decode,
};

/// `InstrumentItem.getInstrument(stack)`: the `instrument` component resolved against `registries`.
pub fn get_instrument(registries: &RegistryAccess, stack: &ItemStack) -> Option<Instrument> {
    match &stack.get(&INSTRUMENT)?.instrument {
        Holder::Reference(id) => registries.get::<Instrument>(keys::INSTRUMENT, id).cloned(),
        Holder::Direct(instrument) => Some(instrument.clone()),
    }
}

struct InstrumentItem;

impl InstrumentItem {
    /// `InstrumentItem.play(level, player, instrument)`.
    fn play(level: &mut ServerLevel, player: &ServerPlayer, instrument: &Instrument) {
        let Some(sound) = holder_sound(&instrument.sound_event) else {
            return;
        };
        let volume = instrument.range / 16.0;
        // `level.playSound(player, player, sound, RECORDS, volume, 1.0F)`: the source entity is the player (taken out of the level)
        level.play_sound_from_entity_at(
            Some(player.id()),
            player.id(),
            player.position(),
            sound,
            SoundSource::Records,
            volume,
            1.0,
        );
        level.game_event_from_entity(
            Some(player.id()),
            GameEvent::INSTRUMENT_PLAY,
            player.position(),
        );
    }
}

impl ItemBehaviour for InstrumentItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let Some(instrument) = get_instrument(&level.registries, &stack) else {
            return InteractionResult::FAIL;
        };
        player.start_using_item(level, hand);
        Self::play(level, player, &instrument);
        if instrument.durability_damage > 0 {
            hurt_and_break(
                &mut stack,
                instrument.durability_damage,
                level,
                player,
                hand.as_equipment_slot(),
            );
            player.set_item_in_hand(hand, stack.clone());
        }
        let cooldown = instrument.use_duration;
        if cooldown > 0.0 {
            player
                .player
                .cooldowns
                .add_cooldown(&stack, mth::floor_f32(cooldown * 20.0));
            player.flush_item_cooldown_packets();
        }
        let item = stack.item();
        player.award_stat_once(level, Stat::Used(item));
        InteractionResult::CONSUME
    }

    fn get_use_duration(&self, stack: &ItemStack) -> i32 {
        with_item_registries(|registries| get_instrument(registries, stack))
            .flatten()
            .map_or(0, |i| mth::floor_f32(i.use_duration * 20.0))
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::TootHorn
    }
}

register_item_class!("InstrumentItem", |_| Box::new(InstrumentItem));
