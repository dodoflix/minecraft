//! `ItemCooldowns` / `ServerItemCooldowns`.
//!
//! The server subclass sends `ClientboundCooldownPacket` from `onCooldownStarted/Ended`; the Rust struct records those
//! events in an outbox that the owning `ServerPlayer` drains and sends (`ServerPlayer::flush_item_cooldown_packets`),
//! in the same order vanilla sends them.

use super::ItemStack;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_protocol::components::USE_COOLDOWN;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
struct CooldownInstance {
    start_time: i32,
    end_time: i32,
}

#[derive(Debug, Default)]
pub struct ItemCooldowns {
    cooldowns: HashMap<Identifier, CooldownInstance>,
    tick_count: i32,
    /// `(group, duration)` of every `onCooldownStarted` (duration) / `onCooldownEnded` (0), oldest first.
    outbox: Vec<(Identifier, i32)>,
}

impl ItemCooldowns {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_on_cooldown(&self, item: &ItemStack) -> bool {
        self.get_cooldown_percent(item, 0.0) > 0.0
    }

    pub fn get_cooldown_percent(&self, item: &ItemStack, a: f32) -> f32 {
        let group = Self::get_cooldown_group(item);
        match self.cooldowns.get(&group) {
            Some(c) => {
                let duration = (c.end_time - c.start_time) as f32;
                let remaining = c.end_time as f32 - (self.tick_count as f32 + a);
                mth::clamp_f32(remaining / duration, 0.0, 1.0)
            }
            None => 0.0,
        }
    }

    /// `tick()`: expired cooldowns end (the iteration order of vanilla's `HashMap` only matters for the packet order of
    /// several groups ending in the same tick; groups are sorted by name here for determinism).
    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        if !self.cooldowns.is_empty() {
            let tick = self.tick_count;
            let mut ended: Vec<Identifier> = self
                .cooldowns
                .iter()
                .filter(|(_, c)| c.end_time <= tick)
                .map(|(k, _)| k.clone())
                .collect();
            ended.sort_by_key(|k| k.to_string());
            for group in ended {
                self.cooldowns.remove(&group);
                self.outbox.push((group, 0));
            }
        }
    }

    /// `getCooldownGroup(item)`: the `use_cooldown` component's group, else the item's registry name.
    pub fn get_cooldown_group(item: &ItemStack) -> Identifier {
        let default_group = item.item().identifier();
        item.get(&USE_COOLDOWN)
            .and_then(|c| c.cooldown_group.clone())
            .unwrap_or(default_group)
    }

    pub fn add_cooldown(&mut self, item: &ItemStack, time: i32) {
        self.add_cooldown_group(Self::get_cooldown_group(item), time);
    }

    pub fn add_cooldown_group(&mut self, group: Identifier, time: i32) {
        self.cooldowns.insert(
            group.clone(),
            CooldownInstance {
                start_time: self.tick_count,
                end_time: self.tick_count.wrapping_add(time),
            },
        );
        self.outbox.push((group, time));
    }

    pub fn remove_cooldown(&mut self, group: &Identifier) {
        self.cooldowns.remove(group);
        self.outbox.push((group.clone(), 0));
    }

    /// Pending `ClientboundCooldownPacket` payloads `(group, duration)`.
    pub fn take_outbox(&mut self) -> Vec<(Identifier, i32)> {
        std::mem::take(&mut self.outbox)
    }

    /// `(group, endTime - tickCount)` of every running cooldown, sorted by group (the scenario dump `cooldowns:`).
    pub fn running(&self) -> Vec<(Identifier, i32)> {
        let mut running: Vec<(Identifier, i32)> = self
            .cooldowns
            .iter()
            .map(|(group, c)| (group.clone(), c.end_time.wrapping_sub(self.tick_count)))
            .collect();
        running.sort_by_key(|(group, _)| group.to_string());
        running
    }
}
