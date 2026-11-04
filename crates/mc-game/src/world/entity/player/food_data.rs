//! `net.minecraft.world.food.FoodData` (state + NBT; `tick` lives with the player tick).
use mc_core::math::mth;
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

    pub fn has_enough_food(&self) -> bool {
        self.food_level as f32 > 6.0
    }

    pub fn needs_food(&self) -> bool {
        self.food_level < 20
    }

    pub fn add_exhaustion(&mut self, amount: f32) {
        self.exhaustion_level = (self.exhaustion_level + amount).min(40.0);
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

/// `Difficulty`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

/// What `FoodData.tick` reads from the level (`getDifficulty()`, `GameRules.NATURAL_HEALTH_REGENERATION`).
#[derive(Clone, Copy, Debug)]
pub struct FoodTickRules {
    pub difficulty: Difficulty,
    pub natural_regeneration: bool,
}

impl Default for FoodTickRules {
    /// Vanilla defaults (`NORMAL`, natural regeneration on); the level's data supplies the real values once it exists.
    fn default() -> Self {
        Self { difficulty: Difficulty::Normal, natural_regeneration: true }
    }
}

/// Effects of a food tick the caller applies to the player.
#[derive(Default, Debug, PartialEq)]
pub struct FoodTickResult {
    pub heal: f32,
    /// `hurtServer(starve, 1.0)`.
    pub starve: bool,
}

impl FoodData {
    /// `tick(player)`.
    pub fn tick(&mut self, rules: FoodTickRules, health: f32, max_health: f32) -> FoodTickResult {
        let mut result = FoodTickResult::default();
        let hurt = health > 0.0 && health < max_health;
        if self.exhaustion_level > 4.0 {
            self.exhaustion_level -= 4.0;
            if self.saturation_level > 0.0 {
                self.saturation_level = (self.saturation_level - 1.0).max(0.0);
            } else if rules.difficulty != Difficulty::Peaceful {
                self.food_level = (self.food_level - 1).max(0);
            }
        }
        if rules.natural_regeneration && self.saturation_level > 0.0 && hurt && self.food_level >= 20 {
            self.tick_timer += 1;
            if self.tick_timer >= 10 {
                let spent = self.saturation_level.min(6.0);
                result.heal = spent / 6.0;
                self.add_exhaustion(spent);
                self.tick_timer = 0;
            }
        } else if rules.natural_regeneration && self.food_level >= 18 && hurt {
            self.tick_timer += 1;
            if self.tick_timer >= 80 {
                result.heal = 1.0;
                self.add_exhaustion(6.0);
                self.tick_timer = 0;
            }
        } else if self.food_level <= 0 {
            self.tick_timer += 1;
            if self.tick_timer >= 80 {
                if health > 10.0 || rules.difficulty == Difficulty::Hard || (health > 1.0 && rules.difficulty == Difficulty::Normal) {
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
