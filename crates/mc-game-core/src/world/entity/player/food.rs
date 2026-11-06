//! The player side of `FoodData.tick`, `Player.causeFoodExhaustion`, `ServerPlayer.tickRegeneration`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_world::level_storage::Difficulty;

impl ServerPlayer {
    /// `Player.isHurt()`.
    pub fn is_hurt(&self) -> bool {
        let l = &self.player.living;
        l.get_health() > 0.0 && l.get_health() < l.get_max_health()
    }

    /// `Player.causeFoodExhaustion(amount)`: invulnerable players (creative/spectator) never tire.
    pub fn cause_food_exhaustion(&mut self, amount: f32) {
        if !self.player.abilities.invulnerable {
            self.player.food_data.add_exhaustion(amount);
        }
    }

    /// `Player.hasEnoughFoodToDoExhaustiveManoeuvres()` (sprinting).
    pub fn has_enough_food_to_do_exhaustive_manoeuvres(&self) -> bool {
        self.player.food_data.has_enough_food() || self.player.abilities.mayfly
    }

    /// `FoodData.tick(player)`: the timers, then `player.heal(..)` / `player.hurtServer(level, starve, 1.0F)`.
    pub fn tick_food(&mut self, level: &mut ServerLevel) {
        let difficulty = level.difficulty();
        let health = self.player.living.get_health();
        let max_health = self.player.living.get_max_health();
        let result = self.player.food_data.tick(
            difficulty,
            level.natural_health_regeneration(),
            health,
            max_health,
        );
        if let Some(amount) = result.heal {
            self.heal(level, amount);
        }
        if result.starve {
            let source = level.damage_sources().starve();
            self.hurt_server(level, &source, 1.0);
        }
    }

    /// `ServerPlayer.tickRegeneration()`: peaceful health, saturation and food regeneration.
    pub fn tick_regeneration(&mut self, level: &mut ServerLevel) {
        if level.difficulty() != Difficulty::Peaceful || !level.natural_health_regeneration() {
            return;
        }
        let tick_count = self.base().tick_count;
        if tick_count % 20 == 0 {
            if self.player.living.get_health() < self.player.living.get_max_health() {
                self.heal(level, 1.0);
            }
            let saturation = self.player.food_data.get_saturation_level();
            if saturation < 20.0 {
                self.player.food_data.set_saturation(saturation + 1.0);
            }
        }
        if tick_count % 10 == 0 && self.player.food_data.needs_food() {
            let food = self.player.food_data.get_food_level();
            self.player.food_data.set_food_level(food.wrapping_add(1));
        }
    }
}
