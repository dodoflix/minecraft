//! `OverworldBiomeBuilder`: the overworld's multi-noise biome parameter table. The entry order matters (the
//! R-tree is built from it and ties resolve through it), so every function keeps vanilla's call order.

use super::climate::{self, Parameter, ParameterPoint};

/// A biome key: the path of a `minecraft:` biome.
pub type BiomeKey = &'static str;

/// `Consumer<Pair<ParameterPoint, ResourceKey<Biome>>>`.
pub type BiomeSink<'a> = &'a mut dyn FnMut(ParameterPoint, BiomeKey);

const fn p(min: f32, max: f32) -> (f32, f32) {
    (min, max)
}

fn span(range: (f32, f32)) -> Parameter {
    Parameter::span(range.0, range.1)
}

const OCEANS: [[BiomeKey; 5]; 2] = [
    [
        "deep_frozen_ocean",
        "deep_cold_ocean",
        "deep_ocean",
        "deep_lukewarm_ocean",
        "warm_ocean",
    ],
    [
        "frozen_ocean",
        "cold_ocean",
        "ocean",
        "lukewarm_ocean",
        "warm_ocean",
    ],
];

const MIDDLE_BIOMES: [[BiomeKey; 5]; 5] = [
    [
        "snowy_plains",
        "snowy_plains",
        "snowy_plains",
        "snowy_taiga",
        "taiga",
    ],
    [
        "plains",
        "plains",
        "forest",
        "taiga",
        "old_growth_spruce_taiga",
    ],
    [
        "flower_forest",
        "plains",
        "forest",
        "birch_forest",
        "dark_forest",
    ],
    ["savanna", "savanna", "forest", "jungle", "jungle"],
    ["desert", "desert", "desert", "desert", "desert"],
];

const MIDDLE_BIOMES_VARIANT: [[Option<BiomeKey>; 5]; 5] = [
    [Some("ice_spikes"), None, Some("snowy_taiga"), None, None],
    [
        Some("dappled_forest"),
        None,
        None,
        None,
        Some("old_growth_pine_taiga"),
    ],
    [
        Some("sunflower_plains"),
        None,
        None,
        Some("old_growth_birch_forest"),
        None,
    ],
    [
        None,
        None,
        Some("plains"),
        Some("sparse_jungle"),
        Some("bamboo_jungle"),
    ],
    [None, None, None, None, None],
];

const PLATEAU_BIOMES: [[BiomeKey; 5]; 5] = [
    [
        "snowy_plains",
        "snowy_plains",
        "snowy_plains",
        "snowy_taiga",
        "snowy_taiga",
    ],
    [
        "meadow",
        "meadow",
        "forest",
        "taiga",
        "old_growth_spruce_taiga",
    ],
    ["meadow", "meadow", "meadow", "meadow", "pale_garden"],
    [
        "savanna_plateau",
        "savanna_plateau",
        "forest",
        "forest",
        "jungle",
    ],
    [
        "badlands",
        "badlands",
        "badlands",
        "wooded_badlands",
        "wooded_badlands",
    ],
];

const PLATEAU_BIOMES_VARIANT: [[Option<BiomeKey>; 5]; 5] = [
    [Some("ice_spikes"), None, None, None, None],
    [
        Some("cherry_grove"),
        None,
        Some("meadow"),
        Some("meadow"),
        Some("old_growth_pine_taiga"),
    ],
    [
        Some("cherry_grove"),
        Some("cherry_grove"),
        Some("forest"),
        Some("birch_forest"),
        None,
    ],
    [None, None, None, None, None],
    [
        Some("eroded_badlands"),
        Some("eroded_badlands"),
        None,
        None,
        None,
    ],
];

const SHATTERED_BIOMES: [[Option<BiomeKey>; 5]; 5] = [
    [
        Some("windswept_gravelly_hills"),
        Some("windswept_gravelly_hills"),
        Some("windswept_hills"),
        Some("windswept_forest"),
        Some("windswept_forest"),
    ],
    [
        Some("windswept_gravelly_hills"),
        Some("windswept_gravelly_hills"),
        Some("windswept_hills"),
        Some("windswept_forest"),
        Some("windswept_forest"),
    ],
    [
        Some("windswept_hills"),
        Some("windswept_hills"),
        Some("windswept_hills"),
        Some("windswept_forest"),
        Some("windswept_forest"),
    ],
    [None, None, None, None, None],
    [None, None, None, None, None],
];

pub struct OverworldBiomeBuilder {
    full_range: Parameter,
    temperatures: [Parameter; 5],
    humidities: [Parameter; 5],
    erosions: [Parameter; 7],
    frozen_range: Parameter,
    unfrozen_range: Parameter,
    mushroom_fields_continentalness: Parameter,
    deep_ocean_continentalness: Parameter,
    ocean_continentalness: Parameter,
    coast_continentalness: Parameter,
    inland_continentalness: Parameter,
    near_inland_continentalness: Parameter,
    mid_inland_continentalness: Parameter,
    far_inland_continentalness: Parameter,
}

impl Default for OverworldBiomeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl OverworldBiomeBuilder {
    pub fn new() -> OverworldBiomeBuilder {
        let temperatures = [
            span(p(-1.0, -0.45)),
            span(p(-0.45, -0.15)),
            span(p(-0.15, 0.2)),
            span(p(0.2, 0.55)),
            span(p(0.55, 1.0)),
        ];
        OverworldBiomeBuilder {
            full_range: Parameter::span(-1.0, 1.0),
            temperatures,
            humidities: [
                span(p(-1.0, -0.35)),
                span(p(-0.35, -0.1)),
                span(p(-0.1, 0.1)),
                span(p(0.1, 0.3)),
                span(p(0.3, 1.0)),
            ],
            erosions: [
                span(p(-1.0, -0.78)),
                span(p(-0.78, -0.375)),
                span(p(-0.375, -0.2225)),
                span(p(-0.2225, 0.05)),
                span(p(0.05, 0.45)),
                span(p(0.45, 0.55)),
                span(p(0.55, 1.0)),
            ],
            frozen_range: temperatures[0],
            unfrozen_range: Parameter::span_of(temperatures[1], temperatures[4]),
            mushroom_fields_continentalness: span(p(-1.2, -1.05)),
            deep_ocean_continentalness: span(p(-1.05, -0.455)),
            ocean_continentalness: span(p(-0.455, -0.19)),
            coast_continentalness: span(p(-0.19, -0.11)),
            inland_continentalness: span(p(-0.11, 0.55)),
            near_inland_continentalness: span(p(-0.11, 0.03)),
            mid_inland_continentalness: span(p(0.03, 0.3)),
            far_inland_continentalness: span(p(0.3, 1.0)),
        }
    }

    /// `addBiomes(biomes)` (the non-debug branch).
    pub fn add_biomes(&self, biomes: BiomeSink) {
        self.add_off_coast_biomes(biomes);
        self.add_inland_biomes(biomes);
        self.add_underground_biomes(biomes);
    }

    fn add_off_coast_biomes(&self, biomes: BiomeSink) {
        self.add_surface_biome(
            biomes,
            self.full_range,
            self.full_range,
            self.mushroom_fields_continentalness,
            self.full_range,
            self.full_range,
            0.0,
            "mushroom_fields",
        );
        for (temperature_index, &temperature) in self.temperatures.iter().enumerate() {
            self.add_surface_biome(
                biomes,
                temperature,
                self.full_range,
                self.deep_ocean_continentalness,
                self.full_range,
                self.full_range,
                0.0,
                OCEANS[0][temperature_index],
            );
            self.add_surface_biome(
                biomes,
                temperature,
                self.full_range,
                self.ocean_continentalness,
                self.full_range,
                self.full_range,
                0.0,
                OCEANS[1][temperature_index],
            );
        }
    }

    fn add_inland_biomes(&self, biomes: BiomeSink) {
        self.add_mid_slice(biomes, Parameter::span(-1.0, -0.93333334));
        self.add_high_slice(biomes, Parameter::span(-0.93333334, -0.7666667));
        self.add_peaks(biomes, Parameter::span(-0.7666667, -0.56666666));
        self.add_high_slice(biomes, Parameter::span(-0.56666666, -0.4));
        self.add_mid_slice(biomes, Parameter::span(-0.4, -0.26666668));
        self.add_low_slice(biomes, Parameter::span(-0.26666668, -0.05));
        self.add_valleys(biomes, Parameter::span(-0.05, 0.05));
        self.add_low_slice(biomes, Parameter::span(0.05, 0.26666668));
        self.add_mid_slice(biomes, Parameter::span(0.26666668, 0.4));
        self.add_high_slice(biomes, Parameter::span(0.4, 0.56666666));
        self.add_peaks(biomes, Parameter::span(0.56666666, 0.7666667));
        self.add_high_slice(biomes, Parameter::span(0.7666667, 0.93333334));
        self.add_mid_slice(biomes, Parameter::span(0.93333334, 1.0));
    }

    fn add_peaks(&self, biomes: BiomeSink, weirdness: Parameter) {
        for temperature_index in 0..self.temperatures.len() {
            let temperature = self.temperatures[temperature_index];
            for humidity_index in 0..self.humidities.len() {
                let humidity = self.humidities[humidity_index];
                let (ti, hi) = (temperature_index, humidity_index);
                let middle_biome = self.pick_middle_biome(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot =
                    self.pick_middle_biome_or_badlands_if_hot(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot_or_slope_if_cold =
                    self.pick_middle_biome_or_badlands_if_hot_or_slope_if_cold(ti, hi, weirdness);
                let plateau_biome = self.pick_plateau_biome(ti, hi, weirdness);
                let shattered_biome = self.pick_shattered_biome(ti, hi, weirdness);
                let shattered_biome_or_windswept_savanna =
                    self.maybe_pick_windswept_savanna_biome(ti, hi, weirdness, shattered_biome);
                let peak_biome = self.pick_peak_biome(ti, hi, weirdness);
                let coast = self.coast_continentalness;
                let near = self.near_inland_continentalness;
                let mid = self.mid_inland_continentalness;
                let far = self.far_inland_continentalness;
                let e = &self.erosions;
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, far),
                    e[0],
                    weirdness,
                    0.0,
                    peak_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, near),
                    e[1],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot_or_slope_if_cold,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[1],
                    weirdness,
                    0.0,
                    peak_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, near),
                    Parameter::span_of(e[2], e[3]),
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[2],
                    weirdness,
                    0.0,
                    plateau_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    mid,
                    e[3],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    far,
                    e[3],
                    weirdness,
                    0.0,
                    plateau_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, far),
                    e[4],
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, near),
                    e[5],
                    weirdness,
                    0.0,
                    shattered_biome_or_windswept_savanna,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[5],
                    weirdness,
                    0.0,
                    shattered_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, far),
                    e[6],
                    weirdness,
                    0.0,
                    middle_biome,
                );
            }
        }
    }

    fn add_high_slice(&self, biomes: BiomeSink, weirdness: Parameter) {
        for temperature_index in 0..self.temperatures.len() {
            let temperature = self.temperatures[temperature_index];
            for humidity_index in 0..self.humidities.len() {
                let humidity = self.humidities[humidity_index];
                let (ti, hi) = (temperature_index, humidity_index);
                let middle_biome = self.pick_middle_biome(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot =
                    self.pick_middle_biome_or_badlands_if_hot(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot_or_slope_if_cold =
                    self.pick_middle_biome_or_badlands_if_hot_or_slope_if_cold(ti, hi, weirdness);
                let plateau_biome = self.pick_plateau_biome(ti, hi, weirdness);
                let shattered_biome = self.pick_shattered_biome(ti, hi, weirdness);
                let middle_biome_or_windswept_savanna =
                    self.maybe_pick_windswept_savanna_biome(ti, hi, weirdness, middle_biome);
                let slope_biome = self.pick_slope_biome(ti, hi, weirdness);
                let peak_biome = self.pick_peak_biome(ti, hi, weirdness);
                let coast = self.coast_continentalness;
                let near = self.near_inland_continentalness;
                let mid = self.mid_inland_continentalness;
                let far = self.far_inland_continentalness;
                let e = &self.erosions;
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    coast,
                    Parameter::span_of(e[0], e[1]),
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    e[0],
                    weirdness,
                    0.0,
                    slope_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[0],
                    weirdness,
                    0.0,
                    peak_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    e[1],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot_or_slope_if_cold,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[1],
                    weirdness,
                    0.0,
                    slope_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, near),
                    Parameter::span_of(e[2], e[3]),
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[2],
                    weirdness,
                    0.0,
                    plateau_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    mid,
                    e[3],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    far,
                    e[3],
                    weirdness,
                    0.0,
                    plateau_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, far),
                    e[4],
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, near),
                    e[5],
                    weirdness,
                    0.0,
                    middle_biome_or_windswept_savanna,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[5],
                    weirdness,
                    0.0,
                    shattered_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, far),
                    e[6],
                    weirdness,
                    0.0,
                    middle_biome,
                );
            }
        }
    }

    fn add_mid_slice(&self, biomes: BiomeSink, weirdness: Parameter) {
        let coast = self.coast_continentalness;
        let near = self.near_inland_continentalness;
        let mid = self.mid_inland_continentalness;
        let far = self.far_inland_continentalness;
        let e = &self.erosions;
        let t = &self.temperatures;
        self.add_surface_biome(
            biomes,
            self.full_range,
            self.full_range,
            coast,
            Parameter::span_of(e[0], e[2]),
            weirdness,
            0.0,
            "stony_shore",
        );
        self.add_surface_biome(
            biomes,
            Parameter::span_of(t[1], t[2]),
            self.full_range,
            Parameter::span_of(near, far),
            e[6],
            weirdness,
            0.0,
            "swamp",
        );
        self.add_surface_biome(
            biomes,
            Parameter::span_of(t[3], t[4]),
            self.full_range,
            Parameter::span_of(near, far),
            e[6],
            weirdness,
            0.0,
            "mangrove_swamp",
        );
        for temperature_index in 0..self.temperatures.len() {
            let temperature = self.temperatures[temperature_index];
            for humidity_index in 0..self.humidities.len() {
                let humidity = self.humidities[humidity_index];
                let (ti, hi) = (temperature_index, humidity_index);
                let middle_biome = self.pick_middle_biome(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot =
                    self.pick_middle_biome_or_badlands_if_hot(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot_or_slope_if_cold =
                    self.pick_middle_biome_or_badlands_if_hot_or_slope_if_cold(ti, hi, weirdness);
                let shattered_biome = self.pick_shattered_biome(ti, hi, weirdness);
                let plateau_biome = self.pick_plateau_biome(ti, hi, weirdness);
                let beach_biome = self.pick_beach_biome(ti, hi);
                let middle_biome_or_windswept_savanna =
                    self.maybe_pick_windswept_savanna_biome(ti, hi, weirdness, middle_biome);
                let shattered_coast_biome = self.pick_shattered_coast_biome(ti, hi, weirdness);
                let slope_biome = self.pick_slope_biome(ti, hi, weirdness);
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(near, far),
                    e[0],
                    weirdness,
                    0.0,
                    slope_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(near, mid),
                    e[1],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot_or_slope_if_cold,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    far,
                    e[1],
                    weirdness,
                    0.0,
                    if temperature_index == 0 {
                        slope_biome
                    } else {
                        plateau_biome
                    },
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    e[2],
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    mid,
                    e[2],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    far,
                    e[2],
                    weirdness,
                    0.0,
                    plateau_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(coast, near),
                    e[3],
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[3],
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
                if weirdness.max < 0 {
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        coast,
                        e[4],
                        weirdness,
                        0.0,
                        beach_biome,
                    );
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        Parameter::span_of(near, far),
                        e[4],
                        weirdness,
                        0.0,
                        middle_biome,
                    );
                } else {
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        Parameter::span_of(coast, far),
                        e[4],
                        weirdness,
                        0.0,
                        middle_biome,
                    );
                }
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    coast,
                    e[5],
                    weirdness,
                    0.0,
                    shattered_coast_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    e[5],
                    weirdness,
                    0.0,
                    middle_biome_or_windswept_savanna,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[5],
                    weirdness,
                    0.0,
                    shattered_biome,
                );
                if weirdness.max < 0 {
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        coast,
                        e[6],
                        weirdness,
                        0.0,
                        beach_biome,
                    );
                } else {
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        coast,
                        e[6],
                        weirdness,
                        0.0,
                        middle_biome,
                    );
                }
                if temperature_index == 0 {
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        Parameter::span_of(near, far),
                        e[6],
                        weirdness,
                        0.0,
                        middle_biome,
                    );
                }
            }
        }
    }

    fn add_low_slice(&self, biomes: BiomeSink, weirdness: Parameter) {
        let coast = self.coast_continentalness;
        let near = self.near_inland_continentalness;
        let mid = self.mid_inland_continentalness;
        let far = self.far_inland_continentalness;
        let e = &self.erosions;
        let t = &self.temperatures;
        self.add_surface_biome(
            biomes,
            self.full_range,
            self.full_range,
            coast,
            Parameter::span_of(e[0], e[2]),
            weirdness,
            0.0,
            "stony_shore",
        );
        self.add_surface_biome(
            biomes,
            Parameter::span_of(t[1], t[2]),
            self.full_range,
            Parameter::span_of(near, far),
            e[6],
            weirdness,
            0.0,
            "swamp",
        );
        self.add_surface_biome(
            biomes,
            Parameter::span_of(t[3], t[4]),
            self.full_range,
            Parameter::span_of(near, far),
            e[6],
            weirdness,
            0.0,
            "mangrove_swamp",
        );
        for temperature_index in 0..self.temperatures.len() {
            let temperature = self.temperatures[temperature_index];
            for humidity_index in 0..self.humidities.len() {
                let humidity = self.humidities[humidity_index];
                let (ti, hi) = (temperature_index, humidity_index);
                let middle_biome = self.pick_middle_biome(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot =
                    self.pick_middle_biome_or_badlands_if_hot(ti, hi, weirdness);
                let middle_biome_or_badlands_if_hot_or_slope_if_cold =
                    self.pick_middle_biome_or_badlands_if_hot_or_slope_if_cold(ti, hi, weirdness);
                let beach_biome = self.pick_beach_biome(ti, hi);
                let middle_biome_or_windswept_savanna =
                    self.maybe_pick_windswept_savanna_biome(ti, hi, weirdness, middle_biome);
                let shattered_coast_biome = self.pick_shattered_coast_biome(ti, hi, weirdness);
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    Parameter::span_of(e[0], e[1]),
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    Parameter::span_of(e[0], e[1]),
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot_or_slope_if_cold,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    Parameter::span_of(e[2], e[3]),
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    Parameter::span_of(e[2], e[3]),
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    coast,
                    Parameter::span_of(e[3], e[4]),
                    weirdness,
                    0.0,
                    beach_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(near, far),
                    e[4],
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    coast,
                    e[5],
                    weirdness,
                    0.0,
                    shattered_coast_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    near,
                    e[5],
                    weirdness,
                    0.0,
                    middle_biome_or_windswept_savanna,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    e[5],
                    weirdness,
                    0.0,
                    middle_biome,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    coast,
                    e[6],
                    weirdness,
                    0.0,
                    beach_biome,
                );
                if temperature_index == 0 {
                    self.add_surface_biome(
                        biomes,
                        temperature,
                        humidity,
                        Parameter::span_of(near, far),
                        e[6],
                        weirdness,
                        0.0,
                        middle_biome,
                    );
                }
            }
        }
    }

    fn add_valleys(&self, biomes: BiomeSink, weirdness: Parameter) {
        let coast = self.coast_continentalness;
        let near = self.near_inland_continentalness;
        let mid = self.mid_inland_continentalness;
        let far = self.far_inland_continentalness;
        let inland = self.inland_continentalness;
        let e = &self.erosions;
        let t = &self.temperatures;
        let full = self.full_range;
        let negative = weirdness.max < 0;
        self.add_surface_biome(
            biomes,
            self.frozen_range,
            full,
            coast,
            Parameter::span_of(e[0], e[1]),
            weirdness,
            0.0,
            if negative {
                "stony_shore"
            } else {
                "frozen_river"
            },
        );
        self.add_surface_biome(
            biomes,
            self.unfrozen_range,
            full,
            coast,
            Parameter::span_of(e[0], e[1]),
            weirdness,
            0.0,
            if negative { "stony_shore" } else { "river" },
        );
        self.add_surface_biome(
            biomes,
            self.frozen_range,
            full,
            near,
            Parameter::span_of(e[0], e[1]),
            weirdness,
            0.0,
            "frozen_river",
        );
        self.add_surface_biome(
            biomes,
            self.unfrozen_range,
            full,
            near,
            Parameter::span_of(e[0], e[1]),
            weirdness,
            0.0,
            "river",
        );
        self.add_surface_biome(
            biomes,
            self.frozen_range,
            full,
            Parameter::span_of(coast, far),
            Parameter::span_of(e[2], e[5]),
            weirdness,
            0.0,
            "frozen_river",
        );
        self.add_surface_biome(
            biomes,
            self.unfrozen_range,
            full,
            Parameter::span_of(coast, far),
            Parameter::span_of(e[2], e[5]),
            weirdness,
            0.0,
            "river",
        );
        self.add_surface_biome(
            biomes,
            self.frozen_range,
            full,
            coast,
            e[6],
            weirdness,
            0.0,
            "frozen_river",
        );
        self.add_surface_biome(
            biomes,
            self.unfrozen_range,
            full,
            coast,
            e[6],
            weirdness,
            0.0,
            "river",
        );
        self.add_surface_biome(
            biomes,
            Parameter::span_of(t[1], t[2]),
            full,
            Parameter::span_of(inland, far),
            e[6],
            weirdness,
            0.0,
            "swamp",
        );
        self.add_surface_biome(
            biomes,
            Parameter::span_of(t[3], t[4]),
            full,
            Parameter::span_of(inland, far),
            e[6],
            weirdness,
            0.0,
            "mangrove_swamp",
        );
        self.add_surface_biome(
            biomes,
            self.frozen_range,
            full,
            Parameter::span_of(inland, far),
            e[6],
            weirdness,
            0.0,
            "frozen_river",
        );
        for temperature_index in 0..self.temperatures.len() {
            let temperature = self.temperatures[temperature_index];
            for humidity_index in 0..self.humidities.len() {
                let humidity = self.humidities[humidity_index];
                let middle_biome_or_badlands_if_hot = self.pick_middle_biome_or_badlands_if_hot(
                    temperature_index,
                    humidity_index,
                    weirdness,
                );
                self.add_surface_biome(
                    biomes,
                    temperature,
                    humidity,
                    Parameter::span_of(mid, far),
                    Parameter::span_of(e[0], e[1]),
                    weirdness,
                    0.0,
                    middle_biome_or_badlands_if_hot,
                );
            }
        }
    }

    fn add_underground_biomes(&self, biomes: BiomeSink) {
        self.add_underground_biome(
            biomes,
            self.full_range,
            self.full_range,
            Parameter::span(0.8, 1.0),
            self.full_range,
            self.full_range,
            0.0,
            "dripstone_caves",
        );
        self.add_underground_biome(
            biomes,
            self.full_range,
            Parameter::span(0.7, 1.0),
            self.full_range,
            self.full_range,
            self.full_range,
            0.0,
            "lush_caves",
        );
        self.add_underground_biome(
            biomes,
            self.full_range,
            self.full_range,
            Parameter::span_of(self.coast_continentalness, self.inland_continentalness),
            Parameter::span_of(self.erosions[5], self.erosions[6]),
            Parameter::span(-1.1, -0.85),
            0.0,
            "sulfur_caves",
        );
        self.add_bottom_biome(
            biomes,
            self.full_range,
            self.full_range,
            self.full_range,
            Parameter::span_of(self.erosions[0], self.erosions[1]),
            self.full_range,
            0.0,
            "deep_dark",
        );
    }

    fn pick_middle_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        if weirdness.max < 0 {
            MIDDLE_BIOMES[temperature_index][humidity_index]
        } else {
            MIDDLE_BIOMES_VARIANT[temperature_index][humidity_index]
                .unwrap_or(MIDDLE_BIOMES[temperature_index][humidity_index])
        }
    }

    fn pick_middle_biome_or_badlands_if_hot(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        if temperature_index == 4 {
            self.pick_badlands_biome(humidity_index, weirdness)
        } else {
            self.pick_middle_biome(temperature_index, humidity_index, weirdness)
        }
    }

    fn pick_middle_biome_or_badlands_if_hot_or_slope_if_cold(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        if temperature_index == 0 {
            self.pick_slope_biome(temperature_index, humidity_index, weirdness)
        } else {
            self.pick_middle_biome_or_badlands_if_hot(temperature_index, humidity_index, weirdness)
        }
    }

    fn maybe_pick_windswept_savanna_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
        underlying_biome: BiomeKey,
    ) -> BiomeKey {
        if temperature_index > 1 && humidity_index < 4 && weirdness.max >= 0 {
            "windswept_savanna"
        } else {
            underlying_biome
        }
    }

    fn pick_shattered_coast_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        let beach_or_middle_biome = if weirdness.max >= 0 {
            self.pick_middle_biome(temperature_index, humidity_index, weirdness)
        } else {
            self.pick_beach_biome(temperature_index, humidity_index)
        };
        self.maybe_pick_windswept_savanna_biome(
            temperature_index,
            humidity_index,
            weirdness,
            beach_or_middle_biome,
        )
    }

    fn pick_beach_biome(&self, temperature_index: usize, _humidity_index: usize) -> BiomeKey {
        if temperature_index == 0 {
            "snowy_beach"
        } else if temperature_index == 4 {
            "desert"
        } else {
            "beach"
        }
    }

    fn pick_badlands_biome(&self, humidity_index: usize, weirdness: Parameter) -> BiomeKey {
        if humidity_index < 2 {
            if weirdness.max < 0 {
                "badlands"
            } else {
                "eroded_badlands"
            }
        } else if humidity_index < 3 {
            "badlands"
        } else {
            "wooded_badlands"
        }
    }

    fn pick_plateau_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        if weirdness.max >= 0
            && let Some(variant) = PLATEAU_BIOMES_VARIANT[temperature_index][humidity_index]
        {
            return variant;
        }
        PLATEAU_BIOMES[temperature_index][humidity_index]
    }

    fn pick_peak_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        if temperature_index <= 2 {
            if weirdness.max < 0 {
                "jagged_peaks"
            } else {
                "frozen_peaks"
            }
        } else if temperature_index == 3 {
            "stony_peaks"
        } else {
            self.pick_badlands_biome(humidity_index, weirdness)
        }
    }

    fn pick_slope_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        if temperature_index >= 3 {
            self.pick_plateau_biome(temperature_index, humidity_index, weirdness)
        } else if humidity_index <= 1 {
            "snowy_slopes"
        } else {
            "grove"
        }
    }

    fn pick_shattered_biome(
        &self,
        temperature_index: usize,
        humidity_index: usize,
        weirdness: Parameter,
    ) -> BiomeKey {
        SHATTERED_BIOMES[temperature_index][humidity_index]
            .unwrap_or_else(|| self.pick_middle_biome(temperature_index, humidity_index, weirdness))
    }

    #[allow(clippy::too_many_arguments)]
    fn add_surface_biome(
        &self,
        biomes: BiomeSink,
        temperature: Parameter,
        humidity: Parameter,
        continentalness: Parameter,
        erosion: Parameter,
        weirdness: Parameter,
        offset: f32,
        biome: BiomeKey,
    ) {
        biomes(
            climate::parameters(
                temperature,
                humidity,
                continentalness,
                erosion,
                Parameter::point(0.0),
                weirdness,
                offset,
            ),
            biome,
        );
        biomes(
            climate::parameters(
                temperature,
                humidity,
                continentalness,
                erosion,
                Parameter::point(1.0),
                weirdness,
                offset,
            ),
            biome,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_underground_biome(
        &self,
        biomes: BiomeSink,
        temperature: Parameter,
        humidity: Parameter,
        continentalness: Parameter,
        erosion: Parameter,
        weirdness: Parameter,
        offset: f32,
        biome: BiomeKey,
    ) {
        biomes(
            climate::parameters(
                temperature,
                humidity,
                continentalness,
                erosion,
                Parameter::span(0.2, 0.9),
                weirdness,
                offset,
            ),
            biome,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_bottom_biome(
        &self,
        biomes: BiomeSink,
        temperature: Parameter,
        humidity: Parameter,
        continentalness: Parameter,
        erosion: Parameter,
        weirdness: Parameter,
        offset: f32,
        biome: BiomeKey,
    ) {
        biomes(
            climate::parameters(
                temperature,
                humidity,
                continentalness,
                erosion,
                Parameter::point(1.1),
                weirdness,
                offset,
            ),
            biome,
        );
    }
}
