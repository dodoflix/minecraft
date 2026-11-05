//! `net.minecraft.world.damagesource.CombatTracker`: the damage log of a living entity and its death messages.

use mc_text::events::{ClickEvent, HoverEvent};
use mc_text::{Component, Style};

use super::combat_entry::CombatEntry;
use super::damage_source::DamageSource;
use super::damage_types::DamageTypeTags;
use super::display_name::entity_display_name;
use super::fall_location::FallLocation;
use crate::registry::typed::damage_type::DeathMessageType;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::item::ItemStackExt as _;

pub const RESET_DAMAGE_STATUS_TIME: i32 = 100;
pub const RESET_COMBAT_STATUS_TIME: i32 = 300;

/// `CommonLinks.INTENTIONAL_GAME_DESIGN_BUG`.
const INTENTIONAL_GAME_DESIGN_BUG: &str = "https://bugs.mojang.com/browse/MCPE-28723";

/// `CombatTracker` state; the `mob` it belongs to is passed to every call.
#[derive(Clone, Debug, Default)]
pub struct CombatTracker {
    entries: Vec<CombatEntry>,
    last_damage_time: i32,
    combat_start_time: i32,
    combat_end_time: i32,
    in_combat: bool,
    taking_damage: bool,
}

/// What a tracker call asks the owner to run afterwards (`mob.onEnterCombat()` / `mob.onLeaveCombat()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CombatEvents {
    pub entered_combat: bool,
    pub left_combat: bool,
}

impl CombatTracker {
    /// `recordDamage(source, damage)`; `mob_alive` is evaluated after `recheckStatus` like vanilla (`isAlive()` does not change in between).
    pub fn record_damage(&mut self, level: &ServerLevel, mob: &dyn Entity, source: &DamageSource, damage: f32) -> CombatEvents {
        let mut events = CombatEvents { entered_combat: false, left_combat: self.recheck_status(mob) };
        let fall_location = FallLocation::get_current_fall_location(level, mob);
        let entry = CombatEntry { source: source.clone(), damage, fall_location, fall_distance: mob.base().fall_distance as f32 };
        self.entries.push(entry);
        let tick_count = mob.base().tick_count;
        self.last_damage_time = tick_count;
        self.taking_damage = true;
        if !self.in_combat && mob.is_alive() && source.causing_is_living() {
            self.in_combat = true;
            self.combat_start_time = tick_count;
            self.combat_end_time = tick_count;
            events.entered_combat = true;
        }
        events
    }

    /// `recheckStatus()`; returns whether `onLeaveCombat()` has to run.
    pub fn recheck_status(&mut self, mob: &dyn Entity) -> bool {
        let reset = if self.in_combat { 300 } else { 100 };
        let tick_count = mob.base().tick_count;
        if self.taking_damage && (!mob.is_alive() || tick_count - self.last_damage_time > reset) {
            let was_in_combat = self.in_combat;
            self.taking_damage = false;
            self.in_combat = false;
            self.combat_end_time = tick_count;
            self.entries.clear();
            return was_in_combat;
        }
        false
    }

    /// `getCombatDuration()`.
    pub fn get_combat_duration(&self, mob: &dyn Entity) -> i32 {
        if self.in_combat { mob.base().tick_count - self.combat_start_time } else { self.combat_end_time - self.combat_start_time }
    }

    pub fn entries(&self) -> &[CombatEntry] {
        &self.entries
    }

    /// `getMostSignificantFall()`.
    fn get_most_significant_fall(&self, level: &ServerLevel) -> Option<&CombatEntry> {
        let mut result: Option<&CombatEntry> = None;
        let mut alternative: Option<&CombatEntry> = None;
        let mut alt_damage = 0.0f32;
        let mut best_fall = 0.0f32;
        for (i, entry) in self.entries.iter().enumerate() {
            let previous = if i > 0 { Some(&self.entries[i - 1]) } else { None };
            let source = &entry.source;
            let is_fake_fall = source.is_tag(level, DamageTypeTags::ALWAYS_MOST_SIGNIFICANT_FALL);
            let fall_distance = if is_fake_fall { f32::MAX } else { entry.fall_distance };
            if (source.is_tag(level, DamageTypeTags::IS_FALL) || is_fake_fall) && fall_distance > 0.0 && (result.is_none() || fall_distance > best_fall) {
                result = if i > 0 { previous } else { Some(entry) };
                best_fall = fall_distance;
            }
            if entry.fall_location.is_some() && (alternative.is_none() || entry.damage > alt_damage) {
                alternative = Some(entry);
                alt_damage = entry.damage;
            }
        }
        if best_fall > 5.0 && result.is_some() {
            result
        } else if alt_damage > 5.0 && alternative.is_some() {
            alternative
        } else {
            None
        }
    }

    /// `getMessageForAssistedFall`.
    fn message_for_assisted_fall(mob_name: &Component, attacker_name: &Component, attacker_main_hand: Option<&crate::world::item::ItemStack>, with_item: &str, without_item: &str) -> Component {
        match attacker_main_hand {
            Some(item) if !item.is_empty() && item.has(mc_data::DataComponentType::CUSTOM_NAME) => {
                Component::translatable(with_item, vec![mob_name.clone().into(), attacker_name.clone().into(), item.get_display_name().into()])
            }
            _ => Component::translatable(without_item, vec![mob_name.clone().into(), attacker_name.clone().into()]),
        }
    }

    /// `getFallMessage(knockOffEntry, killingEntity)`.
    fn fall_message(&self, level: &ServerLevel, mob_name: &Component, knock_off: &CombatEntry, killing_source: &DamageSource) -> Component {
        let knock_off_source = &knock_off.source;
        if !knock_off_source.is_tag(level, DamageTypeTags::IS_FALL) && !knock_off_source.is_tag(level, DamageTypeTags::ALWAYS_MOST_SIGNIFICANT_FALL) {
            let killer = killing_source.causing_info();
            let killer_name = killer.map(|e| e.display_name(level));
            let attacker = knock_off_source.causing_info();
            let attacker_name = attacker.map(|e| e.display_name(level));
            match (&attacker_name, attacker) {
                (Some(name), Some(attacker)) if Some(name) != killer_name.as_ref() => {
                    let hand = attacker.is_living.then_some(&attacker.main_hand_item);
                    Self::message_for_assisted_fall(mob_name, name, hand, "death.fell.assist.item", "death.fell.assist")
                }
                _ => match (killer_name, killer) {
                    (Some(name), Some(killer)) => {
                        let hand = killer.is_living.then_some(&killer.main_hand_item);
                        Self::message_for_assisted_fall(mob_name, &name, hand, "death.fell.finish.item", "death.fell.finish")
                    }
                    _ => Component::translatable("death.fell.killer", vec![mob_name.clone().into()]),
                },
            }
        } else {
            let fall_location = knock_off.fall_location.unwrap_or(FallLocation::GENERIC);
            Component::translatable(fall_location.language_key(), vec![mob_name.clone().into()])
        }
    }

    /// `getDeathMessage()`.
    pub fn get_death_message(&self, level: &ServerLevel, mob: &dyn Entity) -> Component {
        let mob_name = entity_display_name(level, mob);
        let Some(killing_blow) = self.entries.last() else {
            return Component::translatable("death.attack.generic", vec![mob_name.into()]);
        };
        let killing_source = &killing_blow.source;
        let knock_off = self.get_most_significant_fall(level);
        let message_type = killing_source.type_().death_message_type;
        if message_type == DeathMessageType::FallVariants && let Some(knock_off) = knock_off {
            self.fall_message(level, &mob_name, knock_off, killing_source)
        } else if message_type == DeathMessageType::IntentionalGameDesign {
            let death_msg = format!("death.attack.{}", killing_source.get_msg_id());
            let link = mc_text::util::wrap_in_square_brackets(Component::translatable(format!("{death_msg}.link"), vec![]))
                .with_style(&intentional_game_design_style());
            Component::translatable(format!("{death_msg}.message"), vec![mob_name.into(), link.into()])
        } else {
            killing_source.get_localized_death_message(level, mob)
        }
    }
}

fn intentional_game_design_style() -> Style {
    Style::EMPTY
        .with_click_event(Some(ClickEvent::OpenUrl(INTENTIONAL_GAME_DESIGN_BUG.to_owned())))
        .with_hover_event(Some(HoverEvent::show_text(Component::literal("MCPE-28723"))))
}
