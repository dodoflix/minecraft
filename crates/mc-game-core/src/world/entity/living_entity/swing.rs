//! `LivingEntity.swing(hand, animation, sendToSwingingEntity)` and `getModifiedSwingDuration(animation)`.

use crate::server::level::ServerLevel;
use crate::server::level::level_events::encode_packet;
use crate::world::entity::Entity;
use crate::world::item::InteractionHand;
use mc_data::MobEffect;
use mc_protocol::packets::play::{ClientboundSwingAnimationPacket, SwingAnimation};

/// `getModifiedSwingDuration(animation)`: haste/conduit power speed it up, mining fatigue slows it down.
fn get_modified_swing_duration(entity: &dyn Entity, animation: &SwingAnimation) -> i32 {
    let Some(living) = entity.living() else {
        return animation.duration;
    };
    let effects = &living.active_effects;
    // `MobEffectUtil.hasDigSpeed` / `getDigSpeedAmplification`
    let dig = [MobEffect::HASTE, MobEffect::CONDUIT_POWER]
        .iter()
        .filter_map(|e| effects.get(e).map(|i| i.amplifier))
        .max();
    match dig {
        Some(amplifier) => animation.duration - (1 + amplifier),
        None => match effects.get(&MobEffect::MINING_FATIGUE) {
            Some(fatigue) => animation.duration + (1 + fatigue.amplifier) * 2,
            None => animation.duration,
        },
    }
}

/// `LivingEntity.swing(hand, animation, sendToSwingingEntity)`: false when a swing in its first half is running.
pub fn swing(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    hand: InteractionHand,
    animation: SwingAnimation,
    send_to_swinging_entity: bool,
) -> bool {
    let duration = get_modified_swing_duration(entity, &animation);
    let id = entity.id();
    let Some(living) = entity.living_mut() else {
        return false;
    };
    if !living.swing_state.start_if_able(hand, animation, duration) {
        return false;
    }
    let payload = encode_packet(&ClientboundSwingAnimationPacket {
        entity_id: id,
        hand: hand.to_protocol(),
        animation,
    });
    if send_to_swinging_entity {
        level
            .entity_tracker
            .send_to_tracking_players_and_self(id, &payload);
    } else {
        level.entity_tracker.send_to_tracking_players(id, &payload);
    }
    true
}
