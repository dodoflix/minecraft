//! `NoiseGeneratorSettings` (levelgen), `NoiseSettings`, `Aquifer.Config` and the `BlockState` codec shape.

use serde_json::Value;

use mc_core::ident::Identifier;

use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::DensityFunction;
use crate::densityfunction::codec::{
    Fields, LoadResult, density_function_from_json, parse_bool, parse_identifier, parse_int,
    parse_int_range, parse_list, parse_str,
};
use crate::noise_router::NoiseRouter;

/// `DimensionType.Y_SIZE` (`(1 << BlockPos.PACKED_Y_LENGTH) - 32`, 12 packed Y bits).
pub const DIMENSION_Y_SIZE: i32 = (1 << 12) - 32;
/// `DimensionType.MAX_Y`.
pub const DIMENSION_MAX_Y: i32 = (DIMENSION_Y_SIZE >> 1) - 1;
/// `DimensionType.MIN_Y`.
pub const DIMENSION_MIN_Y: i32 = DIMENSION_MAX_Y - DIMENSION_Y_SIZE + 1;

/// `NoiseSettings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseSettings {
    pub min_y: i32,
    pub height: i32,
}

impl NoiseSettings {
    /// `NoiseSettings.CODEC` (ranges + `guardY`).
    fn from_json(value: &Value) -> LoadResult<NoiseSettings> {
        let fields = Fields::new(value)?;
        let min_y = fields.required("min_y", |v| {
            parse_int_range(v, DIMENSION_MIN_Y, DIMENSION_MAX_Y)
        })?;
        let height = fields.required("height", |v| parse_int_range(v, 0, DIMENSION_Y_SIZE))?;
        if min_y + height > DIMENSION_MAX_Y + 1 {
            return Err(LoadError::new(format!(
                "min_y + height cannot be higher than: {}",
                DIMENSION_MAX_Y + 1
            )));
        }
        if height % 16 != 0 {
            return Err(LoadError::new("height has to be a multiple of 16"));
        }
        if min_y % 16 != 0 {
            return Err(LoadError::new("min_y has to be a multiple of 16"));
        }
        Ok(NoiseSettings { min_y, height })
    }
}

/// A `BlockState` as written in data: `Name` plus the explicitly listed `Properties` (unlisted properties
/// keep the block's default state values).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockStateSpec {
    pub name: Identifier,
    pub properties: Vec<(String, String)>,
}

impl BlockStateSpec {
    /// `BlockState.CODEC`: a bare block id (its default state) or `{Name, Properties?}`.
    fn from_json(value: &Value) -> LoadResult<BlockStateSpec> {
        if value.is_string() {
            return Ok(BlockStateSpec {
                name: parse_identifier(value)?,
                properties: Vec::new(),
            });
        }
        let fields = Fields::new(value)?;
        let name = fields.required("Name", parse_identifier)?;
        let properties = fields
            .optional("Properties", |v| {
                v.as_object()
                    .ok_or_else(|| LoadError::new("expected object"))?
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.clone(),
                            parse_str(value).map_err(|e| e.at(key))?.to_owned(),
                        ))
                    })
                    .collect()
            })?
            .unwrap_or_default();
        Ok(BlockStateSpec { name, properties })
    }
}

/// `Aquifer.Config`.
#[derive(Clone, Debug)]
pub struct AquiferConfig {
    pub barrier: DensityFunction,
    pub fluid_level_floodedness: DensityFunction,
    pub fluid_level_spread: DensityFunction,
    pub lava: DensityFunction,
    pub exclusion: DensityFunction,
    pub surface_level: DensityFunction,
}

impl AquiferConfig {
    fn from_json(value: &Value, registries: &WorldgenRegistries) -> LoadResult<AquiferConfig> {
        let fields = Fields::new(value)?;
        let df = |name: &str| fields.required(name, |v| density_function_from_json(v, registries));
        Ok(AquiferConfig {
            barrier: df("barrier")?,
            fluid_level_floodedness: df("fluid_level_floodedness")?,
            fluid_level_spread: df("fluid_level_spread")?,
            lava: df("lava")?,
            exclusion: df("exclusion")?,
            surface_level: df("surface_level")?,
        })
    }
}

/// `NoiseGeneratorSettings.DIRECT_CODEC`.
#[derive(Clone, Debug)]
pub struct NoiseGeneratorSettings {
    pub noise: NoiseSettings,
    pub default_block: BlockStateSpec,
    pub default_fluid: BlockStateSpec,
    pub noise_router: NoiseRouter,
    /// `MaterialRule.HOLDER_CODEC` (a rule reference or inline rule), decoded by the material rule loader.
    pub material_rule: Value,
    /// `SpawnTargetPoint.CODEC.listOf()` as raw JSON.
    pub spawn_target: Value,
    pub sea_level: i32,
    pub disable_mob_generation: bool,
    pub aquifers: Option<AquiferConfig>,
    /// `useLegacyRandomSource` (JSON `legacy_random_source`).
    pub legacy_random_source: bool,
    /// `DebugFunctions` as `(label, function)` entries.
    pub debug_functions: Vec<(String, DensityFunction)>,
}

impl NoiseGeneratorSettings {
    pub fn from_json(
        value: &Value,
        registries: &WorldgenRegistries,
    ) -> LoadResult<NoiseGeneratorSettings> {
        let fields = Fields::new(value)?;
        Ok(NoiseGeneratorSettings {
            noise: fields.required("noise", NoiseSettings::from_json)?,
            default_block: fields.required("default_block", BlockStateSpec::from_json)?,
            default_fluid: fields.required("default_fluid", BlockStateSpec::from_json)?,
            noise_router: fields
                .required("noise_router", |v| NoiseRouter::from_json(v, registries))?,
            material_rule: fields.required("material_rule", |v| Ok(v.clone()))?,
            spawn_target: fields.required("spawn_target", |v| {
                if v.is_array() {
                    Ok(v.clone())
                } else {
                    Err(LoadError::new("expected list"))
                }
            })?,
            sea_level: fields.required("sea_level", parse_int)?,
            disable_mob_generation: fields.required("disable_mob_generation", parse_bool)?,
            aquifers: fields.optional("aquifers", |v| AquiferConfig::from_json(v, registries))?,
            legacy_random_source: fields.required("legacy_random_source", parse_bool)?,
            debug_functions: fields
                .optional("debug_functions", |v| {
                    parse_list(v, |e| debug_function_from_json(e, registries))
                })?
                .unwrap_or_default(),
        })
    }
}

/// `NoiseGeneratorSettings.DebugFunctionEntry.CODEC`.
fn debug_function_from_json(
    value: &Value,
    registries: &WorldgenRegistries,
) -> LoadResult<(String, DensityFunction)> {
    let fields = Fields::new(value)?;
    Ok((
        fields.required("label", |v| parse_str(v).map(str::to_owned))?,
        fields.required("function", |v| density_function_from_json(v, registries))?,
    ))
}
