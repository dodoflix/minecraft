//! `MobEffectUtil`.

use crate::server::level::ServerLevel;
use crate::util::game_modes::GameTypeExt as _;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::MobEffect;
use mc_text::Component;

/// `StringUtil.formatTickDuration(ticks, tickrate)`.
pub fn format_tick_duration(ticks: i32, tickrate: f32) -> String {
    let mut seconds = mth::floor_f32(ticks as f32 / tickrate);
    let mut minutes = seconds / 60;
    seconds %= 60;
    let hours = minutes / 60;
    minutes %= 60;
    if hours > 0 { format!("{hours:02}:{minutes:02}:{seconds:02}") } else { format!("{minutes:02}:{seconds:02}") }
}

/// `formatDuration(instance, scale, tickrate)`.
pub fn format_duration(instance: &MobEffectInstance, scale: f32, tickrate: f32) -> Component {
    if instance.is_infinite_duration() {
        Component::translatable("effect.duration.infinite", Vec::new())
    } else {
        let duration = mth::floor_f32(instance.duration as f32 * scale);
        Component::literal(format_tick_duration(duration, tickrate))
    }
}

/// `hasDigSpeed(mob)`.
pub fn has_dig_speed(mob: &dyn Entity) -> bool {
    mob.has_effect(MobEffect::HASTE) || mob.has_effect(MobEffect::CONDUIT_POWER)
}

/// `getDigSpeedAmplification(mob)`.
pub fn get_dig_speed_amplification(mob: &dyn Entity) -> i32 {
    let a = mob.get_effect(MobEffect::HASTE).map_or(0, |e| e.amplifier);
    let b = mob.get_effect(MobEffect::CONDUIT_POWER).map_or(0, |e| e.amplifier);
    a.max(b)
}

/// `hasWaterBreathing(mob)`.
pub fn has_water_breathing(mob: &dyn Entity) -> bool {
    mob.has_effect(MobEffect::WATER_BREATHING) || mob.has_effect(MobEffect::CONDUIT_POWER) || mob.has_effect(MobEffect::BREATH_OF_THE_NAUTILUS)
}

/// `shouldEffectsRefillAirsupply(mob)`.
pub fn should_effects_refill_air_supply(mob: &dyn Entity) -> bool {
    !mob.has_effect(MobEffect::BREATH_OF_THE_NAUTILUS) || mob.has_effect(MobEffect::WATER_BREATHING) || mob.has_effect(MobEffect::CONDUIT_POWER)
}

/// `Entity.isAlliedTo(other)` for the base team logic (`getTeam().isAlliedTo(other.getTeam())`).
fn is_allied_to(level: &ServerLevel, source: &dyn Entity, other: &dyn Entity) -> bool {
    if source.id() == other.id() {
        return true;
    }
    let scoreboard = level.scoreboard();
    let allied = |a: &dyn Entity, b: &dyn Entity| {
        scoreboard.get_players_team(&a.get_scoreboard_name()).is_some_and(|team| team.is_allied_to(scoreboard.get_players_team(&b.get_scoreboard_name())))
    };
    allied(source, other) || allied(other, source)
}

/// `addEffectToPlayersAround(level, source, position, radius, effectInstance, displayEffectLimit)`.
pub fn add_effect_to_players_around(
    level: &mut ServerLevel,
    source: Option<&dyn Entity>,
    position: Vec3,
    radius: f64,
    effect_instance: &MobEffectInstance,
    display_effect_limit: i32,
) -> Vec<EntityId> {
    let effect = effect_instance.effect;
    let players: Vec<EntityId> = level
        .players()
        .iter()
        .copied()
        .filter(|id| {
            let Some(player) = level.get_entity(*id).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()) else { return false };
            player.game_mode.game_mode_for_player.is_survival()
                && source.is_none_or(|s| !is_allied_to(level, s, player))
                && position.distance_to_sqr(player.position()) < radius * radius
                && player.get_effect(effect).is_none_or(|e| e.amplifier < effect_instance.amplifier || e.ends_within(display_effect_limit - 1))
        })
        .collect();
    let source_id = source.map(|s| s.id());
    for id in &players {
        level.with_entity(*id, |level, player| {
            player.add_effect(level, effect_instance.copy(), source_id);
        });
    }
    players
}
