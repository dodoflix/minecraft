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
