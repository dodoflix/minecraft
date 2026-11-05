//! `net.minecraft.world.food.FoodData` (state, NBT and the food tick; the player side effects are in `food.rs`).
use mc_core::math::{java, mth};
use mc_world::level_storage::Difficulty;
use mc_nbt::CompoundTag;

#[derive(Clone, Debug, PartialEq)]
pub struct FoodData {
    pub food_level: i32,
    pub saturation_level: f32,
    pub exhaustion_level: f32,
    pub tick_timer: i32,
}

impl Default for FoodData {
    fn default() -> Self {
        Self { food_level: 20, saturation_level: 5.0, exhaustion_level: 0.0, tick_timer: 0 }
    }
}

impl FoodData {
    fn add(&mut self, food: i32, saturation: f32) {
        self.food_level = mth::clamp_i32(food.wrapping_add(self.food_level), 0, 20);
        self.saturation_level = mth::clamp_f32(saturation + self.saturation_level, 0.0, self.food_level as f32);
    }

    /// `eat(food, saturationModifier)` (`FoodConstants.saturationByModifier`).
    pub fn eat(&mut self, food: i32, saturation_modifier: f32) {
        self.add(food, food as f32 * saturation_modifier * 2.0);
    }

    /// `eat(FoodProperties)`.
    pub fn eat_properties(&mut self, nutrition: i32, saturation: f32) {
        self.add(nutrition, saturation);
    }

    pub fn get_food_level(&self) -> i32 {
        self.food_level
    }

    pub fn get_saturation_level(&self) -> f32 {
        self.saturation_level
    }

    pub fn set_food_level(&mut self, food: i32) {
        self.food_level = food;
    }

    pub fn set_saturation(&mut self, saturation: f32) {
        self.saturation_level = saturation;
    }

    pub fn has_enough_food(&self) -> bool {
        self.food_level as f32 > 6.0
    }

    pub fn needs_food(&self) -> bool {
        self.food_level < 20
    }

    pub fn add_exhaustion(&mut self, amount: f32) {
        self.exhaustion_level = java::min_f32(self.exhaustion_level + amount, 40.0);
    }

    pub fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.food_level = input.get_int_or("foodLevel", 20);
        self.tick_timer = input.get_int_or("foodTickTimer", 0);
        self.saturation_level = input.get_float_or("foodSaturationLevel", 5.0);
        self.exhaustion_level = input.get_float_or("foodExhaustionLevel", 0.0);
    }

    pub fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_int("foodLevel", self.food_level);
        out.put_int("foodTickTimer", self.tick_timer);
        out.put_float("foodSaturationLevel", self.saturation_level);
        out.put_float("foodExhaustionLevel", self.exhaustion_level);
    }
}

/// What a food tick asks the caller to do to the player (`player.heal(..)` / `player.hurtServer(starve, 1.0)`).
#[derive(Default, Debug, PartialEq)]
pub struct FoodTickResult {
    pub heal: Option<f32>,
    /// `hurtServer(level, damageSources().starve(), 1.0F)`.
    pub starve: bool,
}

impl FoodData {
    /// `tick(player)` up to the player side effects: `health`/`max_health` are the player's (`isHurt()` =
    /// `health > 0 && health < maxHealth`); `heal` is applied after the timer update exactly where vanilla calls it
    /// (nothing in the rest of the tick reads the health).
    pub fn tick(&mut self, difficulty: Difficulty, natural_regeneration: bool, health: f32, max_health: f32) -> FoodTickResult {
        let mut result = FoodTickResult::default();
        let hurt = health > 0.0 && health < max_health;
        if self.exhaustion_level > 4.0 {
            self.exhaustion_level -= 4.0;
            if self.saturation_level > 0.0 {
                self.saturation_level = java::max_f32(self.saturation_level - 1.0, 0.0);
            } else if difficulty != Difficulty::Peaceful {
                self.food_level = self.food_level.saturating_sub(1).max(0);
            }
        }
        if natural_regeneration && self.saturation_level > 0.0 && hurt && self.food_level >= 20 {
            self.tick_timer += 1;
            if self.tick_timer >= 10 {
                let saturation_spent = java::min_f32(self.saturation_level, 6.0);
                result.heal = Some(saturation_spent / 6.0);
                self.add_exhaustion(saturation_spent);
                self.tick_timer = 0;
            }
        } else if natural_regeneration && self.food_level >= 18 && hurt {
            self.tick_timer += 1;
            if self.tick_timer >= 80 {
                result.heal = Some(1.0);
                self.add_exhaustion(6.0);
                self.tick_timer = 0;
            }
        } else if self.food_level <= 0 {
            self.tick_timer += 1;
            if self.tick_timer >= 80 {
                if health > 10.0 || difficulty == Difficulty::Hard || (health > 1.0 && difficulty == Difficulty::Normal) {
                    result.starve = true;
                }
                self.tick_timer = 0;
            }
        } else {
            self.tick_timer = 0;
        }
        result
    }
}
