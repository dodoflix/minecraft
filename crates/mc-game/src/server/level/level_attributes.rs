//! `level.environmentAttributes()` for a [`ServerLevel`]: the typed readers every gameplay consumer uses
//! (`getValue(EnvironmentAttributes.X, pos)` / `getDimensionValue(...)`), plus `isBrightOutside` / `isDarkOutside`.
//!
//! Attribute names are the constants of [`crate::world::attribute::attributes`].

use super::ServerLevel;
use crate::world::attribute::{AttrValue, AttributeEnv, BedRule, TriState};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_world::palette::BiomeId;

impl ServerLevel {
    fn with_attribute_env<R>(&self, f: impl FnOnce(&crate::world::attribute::EnvironmentAttributeSystem, &AttributeEnv<'_>) -> R) -> R {
        let biome_at = |pos: [f64; 3], full_resolution: bool| {
            if full_resolution {
                self.get_biome_id(BlockPos::new(mth::floor_f64(pos[0]), mth::floor_f64(pos[1]), mth::floor_f64(pos[2])))
            } else {
                self.noise_biome_id_at_position(pos[0], pos[1], pos[2])
            }
        };
        let env = AttributeEnv { rain_level: self.get_rain_level(1.0), thunder_level: self.get_thunder_level(1.0), biome_at: &biome_at };
        f(&self.tick_state.attributes, &env)
    }

    /// `BiomeManager.getNoiseBiomeAtPosition(x, y, z)` (`None` while the chunk is not loaded).
    pub fn noise_biome_id_at_position(&self, x: f64, y: f64, z: f64) -> Option<BiomeId> {
        self.tick_state.biomes.as_ref()?;
        let (qx, qy, qz) = (mth::floor_f64(x) >> 2, mth::floor_f64(y) >> 2, mth::floor_f64(z) >> 2);
        self.chunk_now(qx >> 2, qz >> 2).map(|c| c.data.access.get_noise_biome(qx, qy, qz))
    }

    /// `Timelines.OVERWORLD_DAY.getPeriodCount(clockManager)` (24000-tick period of the `minecraft:overworld` clock).
    pub fn overworld_day_period_count(&self) -> i64 {
        let clock = Identifier::parse("minecraft:overworld").expect("valid clock id");
        self.tick_state.attributes.clock_total_ticks(&clock) / 24000
    }

    /// `environmentAttributes().getDimensionValue(attribute)`.
    pub fn environment_dimension_value(&self, attribute: &str) -> AttrValue {
        self.with_attribute_env(|system, env| system.get_dimension_value(attribute, env))
    }

    /// `environmentAttributes().getValue(attribute, pos)`.
    pub fn environment_value(&self, attribute: &str, pos: &Vec3) -> AttrValue {
        self.with_attribute_env(|system, env| system.get_value(attribute, [pos.x, pos.y, pos.z], env))
    }

    /// `getValue(attribute, BlockPos)` = at `Vec3.atCenterOf(pos)`.
    pub fn environment_value_at(&self, attribute: &str, pos: BlockPos) -> AttrValue {
        self.environment_value(attribute, &Vec3::new(f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 0.5, f64::from(pos.z()) + 0.5))
    }

    pub fn attribute_bool(&self, attribute: &str, pos: BlockPos) -> bool {
        self.environment_value_at(attribute, pos).as_bool()
    }

    /// `getValue(attribute, Vec3)` as a boolean.
    pub fn attribute_bool_vec(&self, attribute: &str, pos: &Vec3) -> bool {
        self.environment_value(attribute, pos).as_bool()
    }

    pub fn dimension_bool(&self, attribute: &str) -> bool {
        self.environment_dimension_value(attribute).as_bool()
    }

    pub fn attribute_float(&self, attribute: &str, pos: BlockPos) -> f32 {
        self.environment_value_at(attribute, pos).as_float()
    }

    pub fn dimension_float(&self, attribute: &str) -> f32 {
        self.environment_dimension_value(attribute).as_float()
    }

    pub fn attribute_tri_state(&self, attribute: &str, pos: BlockPos) -> TriState {
        match self.environment_value_at(attribute, pos) {
            AttrValue::TriState(t) => t,
            _ => TriState::Default,
        }
    }

    /// `getValue(BED_RULE | STRAW_BED_RULE, pos)`.
    pub fn attribute_bed_rule(&self, attribute: &str, pos: BlockPos) -> std::sync::Arc<BedRule> {
        match self.environment_value_at(attribute, pos) {
            AttrValue::BedRule(rule) => rule,
            other => panic!("{attribute} is not a bed rule: {other:?}"),
        }
    }

    /// `getValue(VILLAGER_ACTIVITY | BABY_VILLAGER_ACTIVITY, pos)` (the activity's registry id).
    pub fn attribute_activity(&self, attribute: &str, pos: &Vec3) -> Identifier {
        match self.environment_value(attribute, pos) {
            AttrValue::Activity(activity) => activity,
            other => panic!("{attribute} is not an activity: {other:?}"),
        }
    }

    /// `dimensionType().ambientLight()`.
    pub fn ambient_light(&self) -> f32 {
        self.tick_state.ambient_light
    }

    /// `Level.isBrightOutside()`.
    pub fn is_bright_outside(&self) -> bool {
        !self.tick_state.has_fixed_time && self.sky_darken < 4
    }

    /// `Level.isDarkOutside()`.
    pub fn is_dark_outside(&self) -> bool {
        !self.tick_state.has_fixed_time && !self.is_bright_outside()
    }
}
