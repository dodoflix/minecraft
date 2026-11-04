//! `Noises`: the vanilla noise registry keys (all in the `minecraft` namespace) and noise instantiation.
use mc_core::ident::Identifier;
/// `Noises.<NAME>` registry paths (namespace `minecraft`).
pub mod names {
    pub const TEMPERATURE: &str = "temperature";
    pub const VEGETATION: &str = "vegetation";
    pub const CONTINENTALNESS: &str = "continentalness";
    pub const EROSION: &str = "erosion";
    pub const TEMPERATURE_LARGE: &str = "temperature_large";
    pub const VEGETATION_LARGE: &str = "vegetation_large";
    pub const CONTINENTALNESS_LARGE: &str = "continentalness_large";
    pub const EROSION_LARGE: &str = "erosion_large";
    pub const RIDGE: &str = "ridge";
    pub const SHIFT: &str = "offset";
    pub const TEMPERATURE_NETHER: &str = "nether/temperature";
    pub const VEGETATION_NETHER: &str = "nether/vegetation";
    pub const AQUIFER_BARRIER: &str = "aquifer_barrier";
    pub const AQUIFER_FLUID_LEVEL_FLOODEDNESS: &str = "aquifer_fluid_level_floodedness";
    pub const AQUIFER_LAVA: &str = "aquifer_lava";
    pub const AQUIFER_FLUID_LEVEL_SPREAD: &str = "aquifer_fluid_level_spread";
    pub const PILLAR: &str = "pillar";
    pub const PILLAR_RARENESS: &str = "pillar_rareness";
    pub const PILLAR_THICKNESS: &str = "pillar_thickness";
    pub const SPAGHETTI_2D: &str = "spaghetti_2d";
    pub const SPAGHETTI_2D_ELEVATION: &str = "spaghetti_2d_elevation";
    pub const SPAGHETTI_2D_MODULATOR: &str = "spaghetti_2d_modulator";
    pub const SPAGHETTI_2D_THICKNESS: &str = "spaghetti_2d_thickness";
    pub const SPAGHETTI_3D_1: &str = "spaghetti_3d_1";
    pub const SPAGHETTI_3D_2: &str = "spaghetti_3d_2";
    pub const SPAGHETTI_3D_RARITY: &str = "spaghetti_3d_rarity";
    pub const SPAGHETTI_3D_THICKNESS: &str = "spaghetti_3d_thickness";
    pub const SPAGHETTI_ROUGHNESS: &str = "spaghetti_roughness";
    pub const SPAGHETTI_ROUGHNESS_MODULATOR: &str = "spaghetti_roughness_modulator";
    pub const CAVE_ENTRANCE: &str = "cave_entrance";
    pub const CAVE_LAYER: &str = "cave_layer";
    pub const CAVE_CHEESE: &str = "cave_cheese";
    pub const ORE_VEININESS: &str = "ore_veininess";
    pub const ORE_VEIN_A: &str = "ore_vein_a";
    pub const ORE_VEIN_B: &str = "ore_vein_b";
    pub const ORE_GAP: &str = "ore_gap";
    pub const NOODLE: &str = "noodle";
    pub const NOODLE_THICKNESS: &str = "noodle_thickness";
    pub const NOODLE_RIDGE_A: &str = "noodle_ridge_a";
    pub const NOODLE_RIDGE_B: &str = "noodle_ridge_b";
    pub const JAGGED: &str = "jagged";
    pub const SURFACE: &str = "surface";
    pub const SURFACE_SECONDARY: &str = "surface_secondary";
    pub const CLAY_BANDS_OFFSET: &str = "clay_bands_offset";
    pub const BADLANDS_PILLAR: &str = "badlands_pillar";
    pub const BADLANDS_PILLAR_ROOF: &str = "badlands_pillar_roof";
    pub const BADLANDS_SURFACE: &str = "badlands_surface";
    pub const ICEBERG_PILLAR: &str = "iceberg_pillar";
    pub const ICEBERG_PILLAR_ROOF: &str = "iceberg_pillar_roof";
    pub const ICEBERG_SURFACE: &str = "iceberg_surface";
    pub const SULFUR_CAVE_GRADIENT: &str = "sulfur_cave_gradient";
    pub const SWAMP: &str = "surface_swamp";
    pub const CALCITE: &str = "calcite";
    pub const GRAVEL: &str = "gravel";
    pub const POWDER_SNOW: &str = "powder_snow";
    pub const PACKED_ICE: &str = "packed_ice";
    pub const ICE: &str = "ice";
    pub const SOUL_SAND_LAYER: &str = "soul_sand_layer";
    pub const GRAVEL_LAYER: &str = "gravel_layer";
    pub const PATCH: &str = "patch";
    pub const SMALL_PATCH: &str = "small_patch";
    pub const NETHERRACK: &str = "netherrack";
    pub const NETHER_WART: &str = "nether_wart";
    pub const NETHER_STATE_SELECTOR: &str = "nether_state_selector";
}

/// `ResourceKey<NormalNoise>` identifier of a noise path from [`names`].
pub fn key(path: &str) -> Identifier {
    Identifier::with_default_namespace(path)
        .expect("worldgen noise path is a valid identifier path")
}
