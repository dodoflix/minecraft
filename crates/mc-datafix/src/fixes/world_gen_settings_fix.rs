//! `WorldGenSettingsFix`: builds the `WorldGenSettings` compound (seed, flags, `dimensions`) out of the legacy level
//! generator fields (`generatorName`, `generatorOptions`, ...). `default_overworld` and `vanilla_levels` are shared
//! with `MissingDimensionFix`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_core::java_string::JavaString;
use mc_core::math::java::{java_string_hash, parse_int};
use mc_dfu::{Dyn, Dynamic, OptionalDynamic};

#[derive(Clone, Copy)]
struct StructureFeatureConfiguration {
    spacing: i32,
    separation: i32,
    salt: i32,
}

impl StructureFeatureConfiguration {
    /// `serialize(ops)`: the record codec writes `spacing`, `separation`, `salt` in that order.
    fn serialize(&self, like: &OptionalDynamic) -> Dynamic {
        like.create_map([
            (like.create_string("spacing"), like.create_int(self.spacing)),
            (
                like.create_string("separation"),
                like.create_int(self.separation),
            ),
            (like.create_string("salt"), like.create_int(self.salt)),
        ])
    }
}

const fn config(spacing: i32, separation: i32, salt: i32) -> StructureFeatureConfiguration {
    StructureFeatureConfiguration {
        spacing,
        separation,
        salt,
    }
}

/// `DEFAULTS`.
const DEFAULTS: [(&str, StructureFeatureConfiguration); 9] = [
    ("minecraft:village", config(32, 8, 10387312)),
    ("minecraft:desert_pyramid", config(32, 8, 14357617)),
    ("minecraft:igloo", config(32, 8, 14357618)),
    ("minecraft:jungle_pyramid", config(32, 8, 14357619)),
    ("minecraft:swamp_hut", config(32, 8, 14357620)),
    ("minecraft:pillager_outpost", config(32, 8, 165745296)),
    ("minecraft:monument", config(32, 5, 10387313)),
    ("minecraft:endcity", config(20, 11, 10387313)),
    ("minecraft:mansion", config(80, 20, 10387319)),
];

/// `DEFAULTS.get(structure)`; every caller passes a key of the table.
fn default_config(structure: &str) -> StructureFeatureConfiguration {
    DEFAULTS
        .iter()
        .find(|(name, _)| *name == structure)
        .map(|(_, c)| *c)
        .expect("structure with a default configuration")
}

/// The `HashMap<String, StructureFeatureConfiguration>` of `fixFlatStructures`, in `put` order.
type StructureConfigs = Vec<(String, StructureFeatureConfiguration)>;

/// `structureConfig.getOrDefault(structure, DEFAULTS.get(structure))`.
fn config_or_default(configs: &StructureConfigs, structure: &str) -> StructureFeatureConfiguration {
    configs
        .iter()
        .find(|(name, _)| name == structure)
        .map(|(_, c)| *c)
        .unwrap_or_else(|| default_config(structure))
}

/// `structureConfig.put(structure, config)`: a present key keeps its position.
fn put_config(
    configs: &mut StructureConfigs,
    structure: &str,
    new_config: StructureFeatureConfiguration,
) {
    match configs.iter_mut().find(|(name, _)| name == structure) {
        Some(entry) => entry.1 = new_config,
        None => configs.push((structure.to_string(), new_config)),
    }
}

/// `new WorldGenSettingsFix(parent)`.
pub fn new(parent: &Schema) -> DataFix {
    DataFix::new("WorldGenSettingsFix", parent, true, |df| {
        df.fix_type_everywhere_typed(
            "WorldGenSettings building",
            &df.input_schema().get_type(&References::WORLD_GEN_SETTINGS),
            |settings| settings.update_remainder(fix),
        )
    })
}

fn noise(
    seed: i64,
    input: &Dynamic,
    noise_generator_settings: Dynamic,
    biome_source: Dynamic,
) -> Dynamic {
    input.create_map([
        (
            input.create_string("type"),
            input.create_string("minecraft:noise"),
        ),
        (input.create_string("biome_source"), biome_source),
        (input.create_string("seed"), input.create_long(seed)),
        (input.create_string("settings"), noise_generator_settings),
    ])
}

fn vanilla_biome_source(
    input: &Dynamic,
    seed: i64,
    legacy_biome_init_layer: bool,
    large_biomes: bool,
) -> Dynamic {
    let mut entries = vec![
        (
            input.create_string("type"),
            input.create_string("minecraft:vanilla_layered"),
        ),
        (input.create_string("seed"), input.create_long(seed)),
        (
            input.create_string("large_biomes"),
            input.create_boolean(large_biomes),
        ),
    ];
    if legacy_biome_init_layer {
        entries.push((
            input.create_string("legacy_biome_init_layer"),
            input.create_boolean(legacy_biome_init_layer),
        ));
    }
    input.create_map(entries)
}

fn fix(input: Dynamic) -> Dynamic {
    let ops = input.ops();
    let seed = input.get("RandomSeed").as_long_or(0);
    let name: Option<String> = input
        .get("generatorName")
        .as_string()
        .result()
        .map(|n| mc_core::java_case::to_lower_case_root(n.as_lossy_str()));
    let legacy_custom_options: Option<JavaString> = input
        .get("legacy_custom_options")
        .as_string()
        .result()
        .or_else(|| {
            if name.as_deref() == Some("customized") {
                input.get("generatorOptions").as_string().result()
            } else {
                None
            }
        });
    let mut caves = false;
    let generator = match name.as_deref() {
        None | Some("customized") => default_overworld(&input, seed),
        Some("flat") => {
            let flat_settings = input.get("generatorOptions");
            let structure_builder = fix_flat_structures(&flat_settings);
            let layers = flat_settings.get("layers").result().unwrap_or_else(|| {
                input.create_list([
                    input.create_map([
                        (input.create_string("height"), input.create_int(1)),
                        (
                            input.create_string("block"),
                            input.create_string("minecraft:bedrock"),
                        ),
                    ]),
                    input.create_map([
                        (input.create_string("height"), input.create_int(2)),
                        (
                            input.create_string("block"),
                            input.create_string("minecraft:dirt"),
                        ),
                    ]),
                    input.create_map([
                        (input.create_string("height"), input.create_int(1)),
                        (
                            input.create_string("block"),
                            input.create_string("minecraft:grass_block"),
                        ),
                    ]),
                ])
            });
            input.create_map([
                (
                    input.create_string("type"),
                    input.create_string("minecraft:flat"),
                ),
                (
                    input.create_string("settings"),
                    input.create_map([
                        (
                            input.create_string("structures"),
                            input.create_map(structure_builder),
                        ),
                        (input.create_string("layers"), layers),
                        (
                            input.create_string("biome"),
                            input.create_string(
                                flat_settings.get("biome").as_string_or("minecraft:plains"),
                            ),
                        ),
                    ]),
                ),
            ])
        }
        Some("debug_all_block_states") => input.create_map([(
            input.create_string("type"),
            input.create_string("minecraft:debug"),
        )]),
        Some("buffet") => {
            let settings = input.get("generatorOptions");
            let chunk_generator_object = settings.get("chunk_generator");
            let ty = chunk_generator_object
                .get("type")
                .as_string_lossy()
                .result();
            let noise_generator_settings = match ty.as_deref() {
                Some("minecraft:caves") => {
                    caves = true;
                    input.create_string("minecraft:caves")
                }
                Some("minecraft:floating_islands") => {
                    input.create_string("minecraft:floating_islands")
                }
                _ => input.create_string("minecraft:overworld"),
            };
            let biome_source = settings.get("biome_source").result().unwrap_or_else(|| {
                input.create_map([(
                    input.create_string("type"),
                    input.create_string("minecraft:fixed"),
                )])
            });
            let fixed_source = if biome_source
                .get("type")
                .as_string_lossy()
                .result()
                .as_deref()
                == Some("minecraft:fixed")
            {
                let biome = biome_source
                    .get("options")
                    .get("biomes")
                    .as_stream()
                    .into_iter()
                    .next()
                    .and_then(|b| b.as_string().result())
                    .unwrap_or_else(|| "minecraft:ocean".into());
                biome_source
                    .remove("options")
                    .set("biome", input.create_string(biome))
            } else {
                biome_source
            };
            noise(seed, &input, noise_generator_settings, fixed_source)
        }
        Some(other) => {
            let normal = other == "default";
            let legacy_biome_init_layer =
                other == "default_1_1" || normal && input.get("generatorVersion").as_int_or(0) == 0;
            let is_amplified = other == "amplified";
            let large_biomes = other == "largebiomes";
            noise(
                seed,
                &input,
                input.create_string(if is_amplified {
                    "minecraft:amplified"
                } else {
                    "minecraft:overworld"
                }),
                vanilla_biome_source(&input, seed, legacy_biome_init_layer, large_biomes),
            )
        }
    };

    let generate_map_features = input.get("MapFeatures").as_boolean_or(true);
    let generate_bonus_chest = input.get("BonusChest").as_boolean_or(false);
    let mut entries = vec![
        (ops.create_string("seed"), ops.create_long(seed)),
        (
            ops.create_string("generate_features"),
            ops.create_boolean(generate_map_features),
        ),
        (
            ops.create_string("bonus_chest"),
            ops.create_boolean(generate_bonus_chest),
        ),
        (
            ops.create_string("dimensions"),
            vanilla_levels(&input, seed, &generator, caves),
        ),
    ];
    if let Some(o) = legacy_custom_options {
        entries.push((
            ops.create_string("legacy_custom_options"),
            ops.create_string(o),
        ));
    }
    Dynamic::new(ops, ops.create_map(entries))
}

/// `defaultOverworld(input, seed)`.
pub fn default_overworld(input: &Dynamic, seed: i64) -> Dynamic {
    noise(
        seed,
        input,
        input.create_string("minecraft:overworld"),
        vanilla_biome_source(input, seed, false, false),
    )
}

/// `vanillaLevels(input, seed, overworldGenerator, caves)`: the raw `dimensions` value.
pub fn vanilla_levels(
    input: &Dynamic,
    seed: i64,
    overworld_generator: &Dynamic,
    caves: bool,
) -> Dyn {
    let ops = input.ops();
    ops.create_map(vec![
        (
            ops.create_string("minecraft:overworld"),
            ops.create_map(vec![
                (
                    ops.create_string("type"),
                    ops.create_string(format!(
                        "minecraft:overworld{}",
                        if caves { "_caves" } else { "" }
                    )),
                ),
                (
                    ops.create_string("generator"),
                    overworld_generator.value().clone(),
                ),
            ]),
        ),
        (
            ops.create_string("minecraft:the_nether"),
            ops.create_map(vec![
                (
                    ops.create_string("type"),
                    ops.create_string("minecraft:the_nether"),
                ),
                (
                    ops.create_string("generator"),
                    noise(
                        seed,
                        input,
                        input.create_string("minecraft:nether"),
                        input.create_map([
                            (
                                input.create_string("type"),
                                input.create_string("minecraft:multi_noise"),
                            ),
                            (input.create_string("seed"), input.create_long(seed)),
                            (
                                input.create_string("preset"),
                                input.create_string("minecraft:nether"),
                            ),
                        ]),
                    )
                    .into_value(),
                ),
            ]),
        ),
        (
            ops.create_string("minecraft:the_end"),
            ops.create_map(vec![
                (
                    ops.create_string("type"),
                    ops.create_string("minecraft:the_end"),
                ),
                (
                    ops.create_string("generator"),
                    noise(
                        seed,
                        input,
                        input.create_string("minecraft:end"),
                        input.create_map([
                            (
                                input.create_string("type"),
                                input.create_string("minecraft:the_end"),
                            ),
                            (input.create_string("seed"), input.create_long(seed)),
                        ]),
                    )
                    .into_value(),
                ),
            ]),
        ),
    ])
}

fn fix_flat_structures(settings: &OptionalDynamic) -> Vec<(Dynamic, Dynamic)> {
    let mut stronghold_distance = 32;
    let mut stronghold_spread = 3;
    let mut stronghold_count = 128;
    let mut has_stronghold = false;
    let mut structure_config: StructureConfigs = Vec::new();
    if !settings.is_present() {
        has_stronghold = true;
        put_config(
            &mut structure_config,
            "minecraft:village",
            default_config("minecraft:village"),
        );
    }

    if let Some(map) = settings
        .get("structures")
        .flat_map(|d| d.get_map_values())
        .result()
    {
        for (structure_key, value1) in map {
            let Some(options) = value1.get_map_values().result() else {
                continue;
            };
            for (option_key, option_value) in options {
                let structure_name = structure_key.as_string_or("");
                let option_name = option_key.as_string_or("");
                let value = option_value.as_string_or("");
                if structure_name == "stronghold" {
                    has_stronghold = true;
                    match option_name.as_lossy_str() {
                        "distance" => {
                            stronghold_distance =
                                get_int_min(value.as_lossy_str(), stronghold_distance, 1)
                        }
                        "spread" => {
                            stronghold_spread =
                                get_int_min(value.as_lossy_str(), stronghold_spread, 1)
                        }
                        "count" => {
                            stronghold_count =
                                get_int_min(value.as_lossy_str(), stronghold_count, 1)
                        }
                        _ => {}
                    }
                } else {
                    match option_name.as_lossy_str() {
                        "distance" => match structure_name.as_lossy_str() {
                            "village" => set_spacing(
                                &mut structure_config,
                                "minecraft:village",
                                value.as_lossy_str(),
                                9,
                            ),
                            "biome_1" => {
                                for structure in [
                                    "minecraft:desert_pyramid",
                                    "minecraft:igloo",
                                    "minecraft:jungle_pyramid",
                                    "minecraft:swamp_hut",
                                    "minecraft:pillager_outpost",
                                ] {
                                    set_spacing(
                                        &mut structure_config,
                                        structure,
                                        value.as_lossy_str(),
                                        9,
                                    );
                                }
                            }
                            "endcity" => set_spacing(
                                &mut structure_config,
                                "minecraft:endcity",
                                value.as_lossy_str(),
                                1,
                            ),
                            "mansion" => set_spacing(
                                &mut structure_config,
                                "minecraft:mansion",
                                value.as_lossy_str(),
                                1,
                            ),
                            _ => {}
                        },
                        "separation" if structure_name == "oceanmonument" => {
                            let config = config_or_default(&structure_config, "minecraft:monument");
                            // the separation is read into the spacing, as in the original
                            let spacing = get_int_min(value.as_lossy_str(), config.separation, 1);
                            put_config(
                                &mut structure_config,
                                "minecraft:monument",
                                StructureFeatureConfiguration {
                                    spacing,
                                    separation: config.separation,
                                    salt: config.salt,
                                },
                            );
                        }
                        "spacing" if structure_name == "oceanmonument" => {
                            set_spacing(
                                &mut structure_config,
                                "minecraft:monument",
                                value.as_lossy_str(),
                                1,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // `Collectors.toMap` over the `HashMap`'s entries gathers them into a second `HashMap` keyed by `Dynamic`
    let structure_config = hash_map_ordered(
        structure_config,
        |(name, _)| java_string_hash(name),
        HASH_MAP_DEFAULT_CAPACITY,
    );
    let entries: Vec<(Dynamic, Dynamic)> = structure_config
        .into_iter()
        .map(|(name, c)| (settings.create_string(name), c.serialize(settings)))
        .collect();
    let entries = hash_map_ordered(
        entries,
        |(key, _)| key.hash_code(),
        HASH_MAP_DEFAULT_CAPACITY,
    );
    let mut structure_builder = vec![(
        settings.create_string("structures"),
        settings.create_map(entries),
    )];
    if has_stronghold {
        structure_builder.push((
            settings.create_string("stronghold"),
            settings.create_map([
                (
                    settings.create_string("distance"),
                    settings.create_int(stronghold_distance),
                ),
                (
                    settings.create_string("spread"),
                    settings.create_int(stronghold_spread),
                ),
                (
                    settings.create_string("count"),
                    settings.create_int(stronghold_count),
                ),
            ]),
        ));
    }
    structure_builder
}

/// `NumberUtils.toInt(input, def)`.
fn get_int(input: &str, def: i32) -> i32 {
    parse_int(input).unwrap_or(def)
}

fn get_int_min(input: &str, def: i32, min: i32) -> i32 {
    min.max(get_int(input, def))
}

fn set_spacing(configs: &mut StructureConfigs, structure: &str, option_value: &str, min: i32) {
    let config = config_or_default(configs, structure);
    let spacing = get_int_min(option_value, config.spacing, min);
    put_config(
        configs,
        structure,
        StructureFeatureConfiguration {
            spacing,
            separation: config.separation,
            salt: config.salt,
        },
    );
}
