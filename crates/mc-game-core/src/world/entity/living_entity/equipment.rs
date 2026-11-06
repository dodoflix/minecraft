//! `LivingEntity.onEquipItem` and `getArmorCoverPercentage`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::equipment::{EquipmentSlot, EquipmentSlotType};
use crate::world::item::ItemStack;
use mc_core::random::RandomSource;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::components::EQUIPPABLE;
use mc_protocol::packets::play::SoundSource;

/// `LivingEntity.onEquipItem(slot, oldStack, stack)` (server side): equip sound + `EQUIP`/`UNEQUIP` game event when the
/// stack really changed.
pub fn on_equip_item(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    slot: EquipmentSlot,
    old: &ItemStack,
    new: &ItemStack,
) {
    if entity.is_spectator() {
        return;
    }
    if !ItemStack::is_same_item_same_components(old, new) && !entity.base().first_tick {
        let equippable = new.get(&EQUIPPABLE);
        if !entity.base().is_silent() {
            if let Some(eq) = equippable {
                if slot == EquipmentSlot::from_protocol(eq.slot) {
                    let own = eq
                        .equip_sound
                        .name()
                        .and_then(|n| SoundEvent::by_name(&n.to_string()));
                    if let Some(sound) = entity.get_equip_sound_override(slot).or(own) {
                        let seed = entity.base_mut().random.next_long();
                        let p = entity.base().position();
                        let source = if entity.is_player() {
                            SoundSource::Players
                        } else {
                            SoundSource::Neutral
                        };
                        level.play_seeded_sound(None, p.x, p.y, p.z, sound, source, 1.0, 1.0, seed);
                    }
                }
            }
        }
        if entity.does_emit_equip_event(slot) {
            let event = if equippable.is_some() {
                GameEvent::EQUIP
            } else {
                GameEvent::UNEQUIP
            };
            level.game_event_from_entity_ref(&*entity, event, entity.base().position());
        }
    }
}

/// `getArmorCoverPercentage()`.
pub fn get_armor_cover_percentage(entity: &dyn Entity) -> f32 {
    let (mut total, mut count) = (0, 0);
    for slot in EquipmentSlot::VALUES {
        if slot.is_armor() && slot.get_type() == EquipmentSlotType::HumanoidArmor {
            if !entity.get_item_by_slot(slot).is_empty() {
                count += 1;
            }
            total += 1;
        }
    }
    if total > 0 {
        count as f32 / total as f32
    } else {
        0.0
    }
}
