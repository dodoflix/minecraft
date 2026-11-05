//! `net.minecraft.advancements.predicates.FoodPredicate`.

use crate::world::entity::player::food_data::FoodData;
use mc_dfu::Codec;
use mc_protocol::components::{Doubles, Ints};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct FoodPredicate {
    pub level: Ints,
    pub saturation: Doubles,
}

impl FoodPredicate {
    pub fn any() -> FoodPredicate {
        FoodPredicate { level: Ints::any(), saturation: Doubles::any() }
    }

    pub fn codec() -> Codec<FoodPredicate> {
        static C: LazyLock<Codec<FoodPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(FoodPredicate {
                level: Ints::codec().optional_field_of_default("level", Ints::any()),
                saturation: Doubles::codec().optional_field_of_default("saturation", Doubles::any()),
            })
            .codec()
        });
        C.clone()
    }

    pub fn matches(&self, food: &FoodData) -> bool {
        self.level.matches(food.food_level) && self.saturation.matches(f64::from(food.saturation_level))
    }
}
