//! Behaviour of `PotionContents` (the data component value lives in `mc_protocol::components`) and `Potion`:
//! effect lists, colour mixing, `getName`, `applyToLivingEntity`, `onConsume`.

use super::mob_effect::{EffectCause, MobEffectExt};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::effect::MobEffectInstance;
use mc_data::Potion;
use mc_protocol::components::PotionContents;

/// `PotionContents.BASE_POTION_COLOR`.
pub const BASE_POTION_COLOR: i32 = -13083194;

/// `Potion.getEffects()` as entity-side instances (`new MobEffectInstance(effect, duration, amplifier, ambient, visible, showIcon)`).
pub fn potion_effects(potion: Potion) -> Vec<MobEffectInstance> {
    potion.data().effects.iter().map(|e| MobEffectInstance::new(e.effect, e.duration, e.amplifier, e.ambient, e.visible, e.show_icon, None)).collect()
}

/// `PotionContents.getAllEffects()`: the potion's effects, then the custom ones.
pub fn get_all_effects(contents: &PotionContents) -> Vec<MobEffectInstance> {
    let mut all = contents.potion.map(potion_effects).unwrap_or_default();
    all.extend(contents.custom_effects.iter().map(MobEffectInstance::from));
    all
}

/// `PotionContents.forEachEffect(consumer, durationScale)`.
pub fn for_each_effect(contents: &PotionContents, duration_scale: f32, mut consumer: impl FnMut(MobEffectInstance)) {
    if let Some(potion) = contents.potion {
        for effect in potion_effects(potion) {
            consumer(effect.with_scaled_duration(duration_scale));
        }
    }
    for effect in &contents.custom_effects {
        consumer(MobEffectInstance::from(effect).with_scaled_duration(duration_scale));
    }
}

/// `PotionContents.is(potion)`.
pub fn is(contents: &PotionContents, potion: Potion) -> bool {
    is_potion_without_custom_effects(contents) && contents.potion == Some(potion)
}

/// `PotionContents.isPotionWithoutCustomEffects()`.
pub fn is_potion_without_custom_effects(contents: &PotionContents) -> bool {
    contents.potion.is_some() && contents.custom_effects.is_empty()
}

/// `PotionContents.hasEffects()`.
pub fn has_effects(contents: &PotionContents) -> bool {
    !contents.custom_effects.is_empty() || contents.potion.is_some_and(|p| !p.data().effects.is_empty())
}

/// `PotionContents.getColor()`.
pub fn get_color(contents: &PotionContents) -> i32 {
    get_color_or(contents, BASE_POTION_COLOR)
}

/// `PotionContents.getColorOr(defaultColor)`.
pub fn get_color_or(contents: &PotionContents, default_color: i32) -> i32 {
    match contents.custom_color {
        Some(color) => color,
        None => get_color_optional(&get_all_effects(contents)).unwrap_or(default_color),
    }
}

/// `PotionContents.getColorOptional(effects)`: the visible effects' colours weighted by `amplifier + 1`.
pub fn get_color_optional(effects: &[MobEffectInstance]) -> Option<i32> {
    let (mut red, mut green, mut blue, mut total_weight) = (0i32, 0i32, 0i32, 0i32);
    for effect in effects.iter().filter(|e| e.visible) {
        let color = effect.effect.get_color();
        let amplifier = effect.amplifier + 1;
        red += amplifier * ((color >> 16) & 0xFF);
        green += amplifier * ((color >> 8) & 0xFF);
        blue += amplifier * (color & 0xFF);
        total_weight += amplifier;
    }
    (total_weight != 0).then(|| (0xFFu32 << 24) as i32 | ((red / total_weight) << 16) | ((green / total_weight) << 8) | (blue / total_weight))
}

/// `PotionContents.getName(prefix)` as the translation key.
pub fn get_name_key(contents: &PotionContents, prefix: &str) -> String {
    let suffix = contents.custom_name.clone().or_else(|| contents.potion.map(|p| p.data().base_name.to_owned())).unwrap_or_else(|| "empty".to_owned());
    format!("{prefix}{suffix}")
}

/// `PotionContents.applyToLivingEntity(entity, durationScale)`.
pub fn apply_to_living_entity(contents: &PotionContents, level: &mut ServerLevel, entity: &mut dyn Entity, duration_scale: f32) {
    // `Player player = entity instanceof Player ? entity : null`: source and owner are the entity itself
    let cause = entity.is_player().then_some(EffectCause::Mob);
    let mut effects = Vec::new();
    for_each_effect(contents, duration_scale, |e| effects.push(e));
    for effect in effects {
        if effect.effect.is_instantaneous() {
            effect.effect.apply_instantaneous_effect(level, cause, cause, entity, effect.amplifier, 1.0);
        } else {
            entity.add_effect(level, effect, None);
        }
    }
}
