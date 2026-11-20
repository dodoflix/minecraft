//! `net.minecraft.util.datafix.fixes.StructuresBecomeConfiguredFix`: chunk structure starts/references are keyed by
//! configured structure ids instead of structure types.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_collections::JavaHashMap;
use mc_dfu::Dynamic;

/// `Conversion`: `biomeMapping` as `(biomes, result)` groups (lookup only, so the group order is irrelevant).
struct Conversion {
    biome_mapping: &'static [(&'static [&'static str], &'static str)],
    fallback: &'static str,
}

impl Conversion {
    fn trivial(result: &'static str) -> Conversion {
        Conversion {
            biome_mapping: &[],
            fallback: result,
        }
    }

    fn biome_mapped(
        biome_mapping: &'static [(&'static [&'static str], &'static str)],
        fallback: &'static str,
    ) -> Conversion {
        Conversion {
            biome_mapping,
            fallback,
        }
    }

    /// `biomeMapping().get(biome)`.
    fn mapping(&self, biome: &str) -> Option<&'static str> {
        self.biome_mapping
            .iter()
            .find(|(biomes, _)| biomes.contains(&biome))
            .map(|(_, result)| *result)
    }
}

/// `CONVERSION_MAP.get(key)`.
fn conversion(key: &str) -> Option<Conversion> {
    Some(match key {
        "mineshaft" => Conversion::biome_mapped(
            &[(
                &[
                    "minecraft:badlands",
                    "minecraft:eroded_badlands",
                    "minecraft:wooded_badlands",
                ],
                "minecraft:mineshaft_mesa",
            )],
            "minecraft:mineshaft",
        ),
        "shipwreck" => Conversion::biome_mapped(
            &[(
                &["minecraft:beach", "minecraft:snowy_beach"],
                "minecraft:shipwreck_beached",
            )],
            "minecraft:shipwreck",
        ),
        "ocean_ruin" => Conversion::biome_mapped(
            &[(
                &[
                    "minecraft:warm_ocean",
                    "minecraft:lukewarm_ocean",
                    "minecraft:deep_lukewarm_ocean",
                ],
                "minecraft:ocean_ruin_warm",
            )],
            "minecraft:ocean_ruin_cold",
        ),
        "village" => Conversion::biome_mapped(
            &[
                (&["minecraft:desert"], "minecraft:village_desert"),
                (&["minecraft:savanna"], "minecraft:village_savanna"),
                (&["minecraft:snowy_plains"], "minecraft:village_snowy"),
                (&["minecraft:taiga"], "minecraft:village_taiga"),
            ],
            "minecraft:village_plains",
        ),
        "ruined_portal" => Conversion::biome_mapped(
            &[
                (&["minecraft:desert"], "minecraft:ruined_portal_desert"),
                (
                    &[
                        "minecraft:badlands",
                        "minecraft:eroded_badlands",
                        "minecraft:wooded_badlands",
                        "minecraft:windswept_hills",
                        "minecraft:windswept_forest",
                        "minecraft:windswept_gravelly_hills",
                        "minecraft:savanna_plateau",
                        "minecraft:windswept_savanna",
                        "minecraft:stony_shore",
                        "minecraft:meadow",
                        "minecraft:frozen_peaks",
                        "minecraft:jagged_peaks",
                        "minecraft:stony_peaks",
                        "minecraft:snowy_slopes",
                    ],
                    "minecraft:ruined_portal_mountain",
                ),
                (
                    &[
                        "minecraft:bamboo_jungle",
                        "minecraft:jungle",
                        "minecraft:sparse_jungle",
                    ],
                    "minecraft:ruined_portal_jungle",
                ),
                (
                    &[
                        "minecraft:deep_frozen_ocean",
                        "minecraft:deep_cold_ocean",
                        "minecraft:deep_ocean",
                        "minecraft:deep_lukewarm_ocean",
                        "minecraft:frozen_ocean",
                        "minecraft:ocean",
                        "minecraft:cold_ocean",
                        "minecraft:lukewarm_ocean",
                        "minecraft:warm_ocean",
                    ],
                    "minecraft:ruined_portal_ocean",
                ),
            ],
            "minecraft:ruined_portal",
        ),
        "pillager_outpost" => Conversion::trivial("minecraft:pillager_outpost"),
        "mansion" => Conversion::trivial("minecraft:mansion"),
        "jungle_pyramid" => Conversion::trivial("minecraft:jungle_pyramid"),
        "desert_pyramid" => Conversion::trivial("minecraft:desert_pyramid"),
        "igloo" => Conversion::trivial("minecraft:igloo"),
        "swamp_hut" => Conversion::trivial("minecraft:swamp_hut"),
        "stronghold" => Conversion::trivial("minecraft:stronghold"),
        "monument" => Conversion::trivial("minecraft:monument"),
        "fortress" => Conversion::trivial("minecraft:fortress"),
        "endcity" => Conversion::trivial("minecraft:end_city"),
        "buried_treasure" => Conversion::trivial("minecraft:buried_treasure"),
        "nether_fossil" => Conversion::trivial("minecraft:nether_fossil"),
        "bastion_remnant" => Conversion::trivial("minecraft:bastion_remnant"),
        _ => return None,
    })
}

/// `new StructuresBecomeConfiguredFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "StructuresBecomeConfiguredFix",
        output_schema,
        false,
        |df| {
            let chunk_type = df.input_schema().get_type(&References::CHUNK);
            let new_chunk_type = df.input_schema().get_type(&References::CHUNK);
            df.write_fix_and_read(
                "StucturesToConfiguredStructures",
                &chunk_type,
                &new_chunk_type,
                fix,
            )
        },
    )
}

fn fix(chunk: Dynamic) -> Dynamic {
    chunk.update("structures", |structures| {
        structures
            .update("starts", |s| update_starts(s, &chunk))
            .update("References", |r| update_references(r, &chunk))
    })
}

fn new_dynamic_map() -> JavaHashMap<Dynamic, Dynamic> {
    JavaHashMap::new(|k| k.hash_code())
}

fn update_starts(starts: Dynamic, chunk: &Dynamic) -> Dynamic {
    let values = starts.get_map_values().result().unwrap_or_default();
    let mut new_map = new_dynamic_map();
    for (key, start) in values {
        if start.get("id").as_string_or("INVALID") != "INVALID" {
            match find_updated_structure_type(&key, chunk) {
                None => log::warn!(
                    "Encountered unknown structure in datafixer: {}",
                    key.as_string_or("<missing key>")
                ),
                Some(new_key) => {
                    new_map.compute_if_absent(new_key, |new_key| start.set("id", new_key.clone()));
                }
            }
        }
    }
    chunk.create_map(new_map.into_entries())
}

fn update_references(references: Dynamic, chunk: &Dynamic) -> Dynamic {
    let values = references.get_map_values().result().unwrap_or_default();
    let mut new_map = new_dynamic_map();
    for (key, ref_list) in values {
        if !ref_list.as_long_stream().is_empty() {
            match find_updated_structure_type(&key, chunk) {
                None => log::warn!(
                    "Encountered unknown structure in datafixer: {}",
                    key.as_string_or("<missing key>")
                ),
                Some(new_key) => new_map.compute(new_key, |_, old_ref_list| match old_ref_list {
                    None => ref_list.clone(),
                    Some(old_ref_list) => ref_list.create_long_list(
                        old_ref_list
                            .as_long_stream()
                            .into_iter()
                            .chain(ref_list.as_long_stream()),
                    ),
                }),
            }
        }
    }
    chunk.create_map(new_map.into_entries())
}

fn find_updated_structure_type(dynamic_key: &Dynamic, chunk: &Dynamic) -> Option<Dynamic> {
    let key = dynamic_key.as_string_or("UNKNOWN").to_lowercase();
    let conversion = conversion(&key)?;
    let mut resulting_id = conversion.fallback;
    if !conversion.biome_mapping.is_empty()
        && let Some(result) = guess_configuration(chunk, &conversion)
    {
        resulting_id = result;
    }
    Some(chunk.create_string(resulting_id))
}

/// The mapped result seen most often in the section biome palettes (the first one on ties).
fn guess_configuration(chunk: &Dynamic, conversion: &Conversion) -> Option<&'static str> {
    let mut matches: Vec<(&'static str, i32)> = Vec::new();
    for section in chunk.get("sections").as_list(|d| d) {
        for biome in section.get("biomes").get("palette").as_list(|d| d) {
            if let Some(mapping) = conversion.mapping(&biome.as_string_or("")) {
                match matches.iter_mut().find(|(m, _)| *m == mapping) {
                    Some((_, count)) => *count = count.wrapping_add(1),
                    None => matches.push((mapping, 1)),
                }
            }
        }
    }
    matches
        .into_iter()
        .reduce(|a, b| if a.1 >= b.1 { a } else { b })
        .map(|(mapping, _)| mapping)
}
