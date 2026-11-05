//! `SpringFeature`: a single fluid source embedded in rock (lava/water springs).

use mc_game_core::world::level::levelgen::feature::Feature;
use mc_game_core::world::level::levelgen::codec::{BlockSet, Fields, LoadError, LoadResult, TypeEntry, parse_bool, parse_identifier, parse_int};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::fluid::{Fluid, FluidState};
use mc_world::ticks::TickPriority;
use serde_json::Value;
use std::sync::Arc;

pub struct SpringFeature {
    pub state: FluidState,
    pub requires_block_below: bool,
    pub rock_count: i32,
    pub hole_count: i32,
    pub valid_blocks: BlockSet,
}

impl Feature for SpringFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let valid = |level: &dyn WorldGenLevel, pos: BlockPos| self.valid_blocks.contains(level.get_block_state(pos).block());
        if !valid(level, origin.above()) {
            return false;
        }
        if self.requires_block_below && !valid(level, origin.below()) {
            return false;
        }
        let current_state = level.get_block_state(origin);
        if !current_state.is_air() && !self.valid_blocks.contains(current_state.block()) {
            return false;
        }
        let mut placed = 0;
        let rock_count = [origin.west(), origin.east(), origin.north(), origin.south(), origin.below()].into_iter().filter(|&p| valid(level, p)).count() as i32;
        let hole_count = [origin.west(), origin.east(), origin.north(), origin.south(), origin.below()].into_iter().filter(|&p| level.is_empty_block(p)).count() as i32;
        if rock_count == self.rock_count && hole_count == self.hole_count {
            level.set_block_flags(origin, self.state.create_legacy_block(), 2);
            level.schedule_fluid_tick(origin, self.state.fluid(), 0, TickPriority::Normal);
            placed += 1;
        }
        placed > 0
    }
}

/// `FluidState.CODEC`: `{id, properties?}`; like `BlockState.CODEC` the properties are read leniently (a malformed
/// `properties` value, unknown property names and invalid property values are ignored: the default value stays).
pub fn parse_fluid_state(value: &Value) -> LoadResult<FluidState> {
    let fields = Fields::new(value)?;
    let id = fields.required("id", parse_identifier)?;
    let fluid = Fluid::by_name(&id.to_string())
        .ok_or_else(|| LoadError::new(format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:fluid]: {id}")).at("id"))?;
    let mut wanted: Vec<(&'static str, &'static str)> = fluid.default_state().data().properties.to_vec();
    if let Some(Value::Object(map)) = fields.get("properties") {
        for (name, value) in map {
            let Some(value) = value.as_str() else { continue };
            // A property value is valid when some state of this fluid carries it.
            let valid = fluid.states().find_map(|s| s.data().properties.iter().find(|(n, v)| n == name && *v == value).copied());
            if let Some(pair) = valid
                && let Some(slot) = wanted.iter_mut().find(|(n, _)| n == name)
            {
                *slot = pair;
            }
        }
    }
    fluid.states().find(|s| s.data().properties == wanted.as_slice()).ok_or_else(|| LoadError::new(format!("No fluid state of {id} with properties {wanted:?}")))
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static SPRING_FEATURE: TypeEntry<dyn Feature> = TypeEntry {
    name: "spring_feature",
    decode: |fields, _| {
        Ok(Arc::new(SpringFeature {
            state: fields.required("state", parse_fluid_state)?,
            requires_block_below: fields.optional("requires_block_below", parse_bool)?.unwrap_or(true),
            rock_count: fields.optional("rock_count", parse_int)?.unwrap_or(4),
            hole_count: fields.optional("hole_count", parse_int)?.unwrap_or(1),
            valid_blocks: fields.required("valid_blocks", BlockSet::from_json)?,
        }))
    },
};
