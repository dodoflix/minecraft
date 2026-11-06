//! `Player`/`ServerPlayer` experience bookkeeping: `giveExperiencePoints`, `giveExperienceLevels`,
//! `getXpNeededForNextLevel`, `onEnchantmentPerformed`, `setExperiencePoints/Levels`, `getBaseExperienceReward`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_data::SoundEvent;
use mc_protocol::packets::play::SoundSource;

impl ServerPlayer {
    /// `getXpNeededForNextLevel()`.
    pub fn get_xp_needed_for_next_level(&self) -> i32 {
        let level = self.player.experience_level;
        if level >= 30 {
            112i32.wrapping_add(level.wrapping_sub(30).wrapping_mul(9))
        } else if level >= 15 {
            37 + (level - 15) * 5
        } else {
            7 + level * 2
        }
    }

    /// `ServerPlayer.setExperiencePoints(amount)`.
    pub fn set_experience_points(&mut self, amount: i32) {
        let limit = self.get_xp_needed_for_next_level() as f32;
        let max = (limit - 1.0) / limit;
        let points = mth::clamp_f32(amount as f32 / limit, 0.0, max);
        if points != self.player.experience_progress {
            self.player.experience_progress = points;
            self.last_sent_exp = -1;
        }
    }

    /// `ServerPlayer.setExperienceLevels(amount)`.
    pub fn set_experience_levels(&mut self, amount: i32) {
        if amount != self.player.experience_level {
            self.player.experience_level = amount;
            self.last_sent_exp = -1;
        }
    }

    /// `ServerPlayer.giveExperiencePoints(i)` (`Player.giveExperiencePoints` unless `i == 0`).
    pub fn give_experience_points(&mut self, level: &mut ServerLevel, i: i32) {
        if i == 0 {
            return;
        }
        // `increaseScore(i)`
        let score = self.player.get_score();
        self.player.set_score(score.wrapping_add(i));
        self.player.experience_progress += i as f32 / self.get_xp_needed_for_next_level() as f32;
        self.player.total_experience =
            mth::clamp_i32(self.player.total_experience.wrapping_add(i), 0, i32::MAX);
        while self.player.experience_progress < 0.0 {
            let remaining =
                self.player.experience_progress * self.get_xp_needed_for_next_level() as f32;
            if self.player.experience_level > 0 {
                self.give_experience_levels(level, -1);
                self.player.experience_progress =
                    1.0 + remaining / self.get_xp_needed_for_next_level() as f32;
            } else {
                self.give_experience_levels(level, -1);
                self.player.experience_progress = 0.0;
            }
        }
        while self.player.experience_progress >= 1.0 {
            self.player.experience_progress = (self.player.experience_progress - 1.0)
                * self.get_xp_needed_for_next_level() as f32;
            self.give_experience_levels(level, 1);
            self.player.experience_progress /= self.get_xp_needed_for_next_level() as f32;
        }
        self.last_sent_exp = -1;
    }

    /// `ServerPlayer.giveExperienceLevels(amount)` (`Player.giveExperienceLevels` unless `amount == 0`).
    pub fn give_experience_levels(&mut self, level: &mut ServerLevel, amount: i32) {
        if amount == 0 {
            return;
        }
        let p = &mut self.player;
        p.experience_level = p.experience_level.saturating_add(amount);
        if p.experience_level < 0 {
            p.experience_level = 0;
            p.experience_progress = 0.0;
            p.total_experience = 0;
        }
        let tick_count = self.base().tick_count;
        if amount > 0
            && self.player.experience_level % 5 == 0
            && (self.player.last_level_up_time as f32) < tick_count as f32 - 100.0
        {
            let vol = if self.player.experience_level > 30 {
                1.0
            } else {
                self.player.experience_level as f32 / 30.0
            };
            let (x, y, z) = (self.base().x(), self.base().y(), self.base().z());
            level.play_sound_at(
                None,
                x,
                y,
                z,
                SoundEvent::ENTITY_PLAYER_LEVELUP,
                SoundSource::Players,
                vol * 0.75,
                1.0,
            );
            self.player.last_level_up_time = tick_count;
        }
        self.last_sent_exp = -1;
    }

    /// `ServerPlayer.onEnchantmentPerformed(itemStack, enchantmentCost)`.
    pub fn on_enchantment_performed(&mut self, enchantment_cost: i32) {
        let p = &mut self.player;
        p.experience_level = p.experience_level.wrapping_sub(enchantment_cost);
        if p.experience_level < 0 {
            p.experience_level = 0;
            p.experience_progress = 0.0;
            p.total_experience = 0;
        }
        self.player.enchantment_seed = self.player.living.entity.random.next_int();
        self.last_sent_exp = -1;
    }

    /// `Player.getBaseExperienceReward(level)`: what a dying player drops as experience.
    pub fn get_base_experience_reward(&self, keep_inventory: bool) -> i32 {
        if !keep_inventory && !self.is_spectator() {
            (self.player.experience_level.wrapping_mul(7)).min(100)
        } else {
            0
        }
    }
}
