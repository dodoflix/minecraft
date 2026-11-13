//! `FishingRodItem`: `use` casts a `FishingHook`, or reels in the one the player already cast (the rod takes the damage
//! the hook reports).

use super::enchantment::actors::Actors;
use super::enchantment_helper::{get_fishing_luck_bonus, get_fishing_time_reduction};
use super::{InteractionHand, InteractionResult, ItemBehaviour, hurt_and_break};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::projectile::fishing_hook::FishingHook;
use crate::world::entity::projectile::spawn::spawn_projectile_plain;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::LevelAccessor;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::SoundSource;

struct FishingRodItem;

/// `level.playSound(null, player.getX(), player.getY(), player.getZ(), sound, NEUTRAL, volume,
/// 0.4F / (level.getRandom().nextFloat() * 0.4F + 0.8F))`.
fn play_sound(level: &mut ServerLevel, player: &ServerPlayer, sound: SoundEvent, volume: f32) {
    let pitch = 0.4 / (level.random().next_float() * 0.4 + 0.8);
    let p = player.base();
    level.play_sound_at(
        None,
        p.x(),
        p.y(),
        p.z(),
        sound,
        SoundSource::Neutral,
        volume,
        pitch,
    );
}

impl ItemBehaviour for FishingRodItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let hook = player.player.fishing.filter(|&id| {
            level
                .get_entity(id)
                .is_some_and(|e| e.as_any().is::<FishingHook>() && !e.is_removed())
        });
        if let Some(hook) = hook {
            let dmg = level
                .with_entity(hook, |level, e| {
                    e.as_any_mut()
                        .downcast_mut::<FishingHook>()
                        .map_or(0, |hook| hook.retrieve(level, player, &stack))
                })
                .unwrap_or(0);
            hurt_and_break(&mut stack, dmg, level, player, hand.as_equipment_slot());
            player.set_item_in_hand(hand, stack.clone());
            play_sound(
                level,
                player,
                SoundEvent::ENTITY_FISHING_BOBBER_RETRIEVE,
                1.0,
            );
            // a rod that broke is the empty stack by now: it has no use effects
            player.cause_use_vibration(level, &stack, GameEvent::ITEM_INTERACT_FINISH);
        } else {
            play_sound(level, player, SoundEvent::ENTITY_FISHING_BOBBER_THROW, 0.5);
            let id = player.id();
            let lure_speed =
                (get_fishing_time_reduction(level, &mut Actors::of(&mut *player), &stack, id)
                    * 20.0) as i32;
            let luck = get_fishing_luck_bonus(level, &mut Actors::of(&mut *player), &stack, id);
            let hook = FishingHook::with_owner(level, &*player, luck, lure_speed);
            player.player.fishing = Some(hook.id());
            spawn_projectile_plain(level, Box::new(hook), &stack);
            player.award_stat_once(level, crate::stats::Stat::Used(stack.item()));
            player.cause_use_vibration(level, &stack, GameEvent::ITEM_INTERACT_START);
        }
        InteractionResult::SUCCESS
    }
}

register_item_class!("FishingRodItem", |_| Box::new(FishingRodItem));
