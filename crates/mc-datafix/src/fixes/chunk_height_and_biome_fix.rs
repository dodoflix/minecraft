//! `ChunkHeightAndBiomeFix`: raises the world height of the overworld by four sections below and four above and
//! moves the biomes into paletted containers per section.

use crate::datafix::DataFix;
use crate::finder::OpticFinder;
use crate::fixes::chunk_proto_tick_list_fix::PoorMansPalettedContainer;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::read_typed_or_throw;
use mc_core::math::java::parse_int;
use mc_dfu::Dynamic;
use std::collections::HashSet;

const HEIGHTMAP_TYPES: [&str; 7] = [
    "WORLD_SURFACE_WG",
    "WORLD_SURFACE",
    "WORLD_SURFACE_IGNORE_SNOW",
    "OCEAN_FLOOR_WG",
    "OCEAN_FLOOR",
    "MOTION_BLOCKING",
    "MOTION_BLOCKING_NO_LEAVES",
];
const STATUS_IS_OR_AFTER_SURFACE: [&str; 8] = [
    "surface",
    "carvers",
    "liquid_carvers",
    "features",
    "light",
    "spawn",
    "heightmaps",
    "full",
];
const STATUS_IS_OR_AFTER_NOISE: [&str; 9] = [
    "noise",
    "surface",
    "carvers",
    "liquid_carvers",
    "features",
    "light",
    "spawn",
    "heightmaps",
    "full",
];

const BIOMES_BY_ID: [(i32, &str); 87] = [
    (0, "minecraft:ocean"),
    (1, "minecraft:plains"),
    (2, "minecraft:desert"),
    (3, "minecraft:mountains"),
    (4, "minecraft:forest"),
    (5, "minecraft:taiga"),
    (6, "minecraft:swamp"),
    (7, "minecraft:river"),
    (8, "minecraft:nether_wastes"),
    (9, "minecraft:the_end"),
    (10, "minecraft:frozen_ocean"),
    (11, "minecraft:frozen_river"),
    (12, "minecraft:snowy_tundra"),
    (13, "minecraft:snowy_mountains"),
    (14, "minecraft:mushroom_fields"),
    (15, "minecraft:mushroom_field_shore"),
    (16, "minecraft:beach"),
    (17, "minecraft:desert_hills"),
    (18, "minecraft:wooded_hills"),
    (19, "minecraft:taiga_hills"),
    (20, "minecraft:mountain_edge"),
    (21, "minecraft:jungle"),
    (22, "minecraft:jungle_hills"),
    (23, "minecraft:jungle_edge"),
    (24, "minecraft:deep_ocean"),
    (25, "minecraft:stone_shore"),
    (26, "minecraft:snowy_beach"),
    (27, "minecraft:birch_forest"),
    (28, "minecraft:birch_forest_hills"),
    (29, "minecraft:dark_forest"),
    (30, "minecraft:snowy_taiga"),
    (31, "minecraft:snowy_taiga_hills"),
    (32, "minecraft:giant_tree_taiga"),
    (33, "minecraft:giant_tree_taiga_hills"),
    (34, "minecraft:wooded_mountains"),
    (35, "minecraft:savanna"),
    (36, "minecraft:savanna_plateau"),
    (37, "minecraft:badlands"),
    (38, "minecraft:wooded_badlands_plateau"),
    (39, "minecraft:badlands_plateau"),
    (40, "minecraft:small_end_islands"),
    (41, "minecraft:end_midlands"),
    (42, "minecraft:end_highlands"),
    (43, "minecraft:end_barrens"),
    (44, "minecraft:warm_ocean"),
    (45, "minecraft:lukewarm_ocean"),
    (46, "minecraft:cold_ocean"),
    (47, "minecraft:deep_warm_ocean"),
    (48, "minecraft:deep_lukewarm_ocean"),
    (49, "minecraft:deep_cold_ocean"),
    (50, "minecraft:deep_frozen_ocean"),
    (127, "minecraft:the_void"),
    (129, "minecraft:sunflower_plains"),
    (130, "minecraft:desert_lakes"),
    (131, "minecraft:gravelly_mountains"),
    (132, "minecraft:flower_forest"),
    (133, "minecraft:taiga_mountains"),
    (134, "minecraft:swamp_hills"),
    (140, "minecraft:ice_spikes"),
    (149, "minecraft:modified_jungle"),
    (151, "minecraft:modified_jungle_edge"),
    (155, "minecraft:tall_birch_forest"),
    (156, "minecraft:tall_birch_hills"),
    (157, "minecraft:dark_forest_hills"),
    (158, "minecraft:snowy_taiga_mountains"),
    (160, "minecraft:giant_spruce_taiga"),
    (161, "minecraft:giant_spruce_taiga_hills"),
    (162, "minecraft:modified_gravelly_mountains"),
    (163, "minecraft:shattered_savanna"),
    (164, "minecraft:shattered_savanna_plateau"),
    (165, "minecraft:eroded_badlands"),
    (166, "minecraft:modified_wooded_badlands_plateau"),
    (167, "minecraft:modified_badlands_plateau"),
    (168, "minecraft:bamboo_jungle"),
    (169, "minecraft:bamboo_jungle_hills"),
    (170, "minecraft:soul_sand_valley"),
    (171, "minecraft:crimson_forest"),
    (172, "minecraft:warped_forest"),
    (173, "minecraft:basalt_deltas"),
    (174, "minecraft:dripstone_caves"),
    (175, "minecraft:lush_caves"),
    (177, "minecraft:meadow"),
    (178, "minecraft:grove"),
    (179, "minecraft:snowy_slopes"),
    (180, "minecraft:snowcapped_peaks"),
    (181, "minecraft:lofty_peaks"),
    (182, "minecraft:stony_peaks"),
];

const BLOCKS_BEFORE_FEATURE_STATUS: [&str; 34] = [
    "minecraft:air",
    "minecraft:basalt",
    "minecraft:bedrock",
    "minecraft:blackstone",
    "minecraft:calcite",
    "minecraft:cave_air",
    "minecraft:coarse_dirt",
    "minecraft:crimson_nylium",
    "minecraft:dirt",
    "minecraft:end_stone",
    "minecraft:grass_block",
    "minecraft:gravel",
    "minecraft:ice",
    "minecraft:lava",
    "minecraft:mycelium",
    "minecraft:nether_wart_block",
    "minecraft:netherrack",
    "minecraft:orange_terracotta",
    "minecraft:packed_ice",
    "minecraft:podzol",
    "minecraft:powder_snow",
    "minecraft:red_sand",
    "minecraft:red_sandstone",
    "minecraft:sand",
    "minecraft:sandstone",
    "minecraft:snow_block",
    "minecraft:soul_sand",
    "minecraft:soul_soil",
    "minecraft:stone",
    "minecraft:terracotta",
    "minecraft:warped_nylium",
    "minecraft:warped_wart_block",
    "minecraft:water",
    "minecraft:white_terracotta",
];

/// `new ChunkHeightAndBiomeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ChunkHeightAndBiomeFix", output_schema, true, |df| {
        let old_chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_finder = old_chunk_type.find_field("Level");
        let sections_finder = level_finder.ty().find_field("Sections");
        let chunk_type = df.output_schema().get_type(&References::CHUNK);
        let level_type = chunk_type.find_field_type("Level");
        let sections_type = level_type.find_field_type("Sections");
        df.fix_type_everywhere_typed_new(
            "ChunkHeightAndBiomeFix",
            &old_chunk_type,
            &chunk_type,
            move |chunk| {
                let context = chunk.get_remainder().get("__context");
                let dimension = context
                    .get("dimension")
                    .as_string_lossy()
                    .result()
                    .unwrap_or_default();
                let generator = context
                    .get("generator")
                    .as_string_lossy()
                    .result()
                    .unwrap_or_default();
                chunk.update_typed_as(&level_finder, &level_type, |level| {
                    fix_level(
                        level,
                        &dimension,
                        &generator,
                        &sections_finder,
                        &sections_type,
                    )
                })
            },
        )
    })
}

fn fix_level(
    level: Typed,
    dimension: &str,
    generator: &str,
    sections_finder: &OpticFinder,
    sections_type: &crate::ty::Type,
) -> Typed {
    let tag = level.get_remainder();
    let is_overworld = dimension == "minecraft:overworld";
    let min_section = if is_overworld { -4 } else { 0 };
    let mut was_increased_height = false;
    let biome_containers =
        get_biome_containers(&tag, is_overworld, min_section, &mut was_increased_height);
    let air_container = make_paletted_container(&tag.create_list([tag.create_map([(
        tag.create_string("Name"),
        tag.create_string("minecraft:air"),
    )])]));
    let mut blocks_in_chunk: HashSet<String> = HashSet::new();
    let mut bedrock_section_blocks: Option<Dynamic> = None;
    let level = level.update_typed_as(sections_finder, sections_type, |sections| {
        let mut done_sections: HashSet<i32> = HashSet::new();
        let dynamic = sections.write().result().unwrap_or_else(|| {
            panic!("java.lang.IllegalStateException: Malformed Chunk.Level.Sections")
        });
        let mut sections_list: Vec<Dynamic> = Vec::new();
        for section in dynamic.as_stream() {
            let section_y = section.get("Y").as_int_or(0);
            let block_states_container = section
                .get("Palette")
                .result()
                .and_then(|palette| {
                    for block_state in palette.as_stream() {
                        blocks_in_chunk
                            .insert(block_state.get("Name").as_string_or_lossy("minecraft:air"));
                    }
                    section.get("BlockStates").result().map(|block_states| {
                        make_optimized_paletted_container(&palette, &block_states)
                    })
                })
                .unwrap_or_else(|| air_container.clone());
            let mut result = section.clone();
            let section_y_index = section_y.wrapping_sub(min_section);
            if let Some(biomes) = usize::try_from(section_y_index)
                .ok()
                .and_then(|i| biome_containers.get(i))
            {
                result = section.set("biomes", biomes.clone());
            }
            done_sections.insert(section_y);
            if section.get("Y").as_int_or(i32::MAX) == 0 {
                bedrock_section_blocks = Some(block_states_container.clone());
            }
            sections_list.push(
                result
                    .set("block_states", block_states_container)
                    .remove("Palette")
                    .remove("BlockStates"),
            );
        }
        for (section_index, biomes) in biome_containers.iter().enumerate() {
            let section_y = section_index as i32 + min_section;
            if done_sections.insert(section_y) {
                let mut section =
                    tag.create_map([(tag.create_string("Y"), tag.create_int(section_y))]);
                section = section.set("block_states", air_container.clone());
                section = section.set("biomes", biomes.clone());
                sections_list.push(section);
            }
        }
        read_typed_or_throw(sections_type, &tag.create_list(sections_list), false)
    });
    level.update_remainder(|chunk_tag| {
        let chunk_tag = if is_overworld {
            predict_chunk_status_before_surface(chunk_tag, &mut blocks_in_chunk)
        } else {
            chunk_tag
        };
        update_chunk_tag(
            chunk_tag,
            is_overworld,
            was_increased_height,
            generator == "minecraft:noise",
            bedrock_section_blocks.as_ref(),
        )
    })
}

fn predict_chunk_status_before_surface(
    chunk_tag: Dynamic,
    blocks_in_chunk: &mut HashSet<String>,
) -> Dynamic {
    chunk_tag.update("Status", |status_dynamic| {
        let status = status_dynamic.as_string_or("empty");
        if STATUS_IS_OR_AFTER_SURFACE.contains(&status.as_lossy_str()) {
            status_dynamic
        } else {
            blocks_in_chunk.remove("minecraft:air");
            let has_non_air_blocks = !blocks_in_chunk.is_empty();
            blocks_in_chunk.retain(|b| !BLOCKS_BEFORE_FEATURE_STATUS.contains(&b.as_str()));
            let has_feature_blocks = !blocks_in_chunk.is_empty();
            if has_feature_blocks {
                status_dynamic.create_string("liquid_carvers")
            } else if status == "noise" || has_non_air_blocks {
                status_dynamic.create_string("noise")
            } else if status == "biomes" {
                status_dynamic.create_string("structure_references")
            } else {
                status_dynamic
            }
        }
    })
}

fn get_biome_containers(
    tag: &Dynamic,
    increase_height: bool,
    min_section: i32,
    was_increased_height: &mut bool,
) -> Vec<Dynamic> {
    let length = if increase_height { 24 } else { 16 };
    let mut biome_containers: Vec<Option<Dynamic>> = vec![None; length];
    let old_biomes: Option<Vec<i32>> = tag.get("Biomes").as_int_stream_opt().result();
    match old_biomes.as_deref() {
        Some(old) if old.len() == 1536 => {
            *was_increased_height = true;
            for section_y_index in 0..24usize {
                let container =
                    make_biome_container(tag, |i| get_old_biome(old, section_y_index * 64 + i));
                match biome_containers.get_mut(section_y_index) {
                    Some(slot) => *slot = Some(container),
                    None => panic!(
                        "java.lang.ArrayIndexOutOfBoundsException: Index {section_y_index} out of bounds for length {length}"
                    ),
                }
            }
        }
        Some(old) if old.len() == 1024 => {
            for section_y in 0..16i32 {
                let section_y_index = (section_y - min_section) as usize;
                biome_containers[section_y_index] = Some(make_biome_container(tag, |i| {
                    get_old_biome(old, section_y as usize * 64 + i)
                }));
            }
            if increase_height {
                let below_world_biomes = make_biome_container(tag, |i| get_old_biome(old, i % 16));
                let above_world_biomes =
                    make_biome_container(tag, |i| get_old_biome(old, i % 16 + 1008));
                for slot in &mut biome_containers[..4] {
                    *slot = Some(below_world_biomes.clone());
                }
                for slot in &mut biome_containers[20..24] {
                    *slot = Some(above_world_biomes.clone());
                }
            }
        }
        _ => {
            let plains =
                make_paletted_container(&tag.create_list([tag.create_string("minecraft:plains")]));
            biome_containers.fill(Some(plains));
        }
    }
    biome_containers
        .into_iter()
        .map(|c| c.expect("every biome container is set"))
        .collect()
}

fn get_old_biome(old_biomes: &[i32], index: usize) -> i32 {
    old_biomes[index] & 0xFF
}

fn update_chunk_tag(
    mut chunk_tag: Dynamic,
    is_overworld: bool,
    was_increased_height: bool,
    needs_blending_and_upgrade: bool,
    bedrock_section_blocks: Option<&Dynamic>,
) -> Dynamic {
    chunk_tag = chunk_tag.remove("Biomes");
    if !is_overworld {
        return update_carving_masks(&chunk_tag, 16, 0);
    }
    if was_increased_height {
        return update_carving_masks(&chunk_tag, 24, 0);
    }
    chunk_tag = update_heightmaps(chunk_tag);
    chunk_tag = add_padding_entries(chunk_tag, "LiquidsToBeTicked");
    chunk_tag = add_padding_entries(chunk_tag, "PostProcessing");
    chunk_tag = add_padding_entries(chunk_tag, "ToBeTicked");
    chunk_tag = update_carving_masks(&chunk_tag, 24, 4);
    chunk_tag = chunk_tag.update("UpgradeData", shift_upgrade_data);
    if !needs_blending_and_upgrade {
        return chunk_tag;
    }
    let Some(status) = chunk_tag.get("Status").result() else {
        return chunk_tag;
    };
    let last_status = status.as_string_or("");
    if last_status == "empty" {
        return chunk_tag;
    }
    chunk_tag = chunk_tag.set(
        "blending_data",
        chunk_tag.create_map([(
            chunk_tag.create_string("old_noise"),
            chunk_tag
                .create_boolean(STATUS_IS_OR_AFTER_NOISE.contains(&last_status.as_lossy_str())),
        )]),
    );
    // `SharedConstants.DEBUG_DISABLE_BELOW_ZERO_RETROGENERATION` is a debug flag that is off
    if let Some(container) = bedrock_section_blocks {
        let poor_mans_paletted_container = PoorMansPalettedContainer::new(
            container.get("palette").as_list(|d| d),
            container.get("data").as_long_stream(),
        );
        let mut missing_bedrock = [0u64; 4];
        let mut has_any_bedrock = last_status == "noise";
        for z in 0..16 {
            for x in 0..16 {
                let block_state = poor_mans_paletted_container.get(x, 0, z);
                let name = block_state.map(|b| b.get("Name").as_string_or_lossy(""));
                let is_bedrock = name.as_deref() == Some("minecraft:bedrock");
                let is_air = name.as_deref() == Some("minecraft:air");
                if is_air {
                    let bit = (z * 16 + x) as usize;
                    missing_bedrock[bit / 64] |= 1u64 << (bit % 64);
                }
                has_any_bedrock |= is_bedrock;
            }
        }
        let cardinality: u32 = missing_bedrock.iter().map(|w| w.count_ones()).sum();
        if has_any_bedrock && cardinality != 256 {
            let target_status = if last_status == "full" {
                chunk_tag.create_string("heightmaps")
            } else {
                status
            };
            let used = missing_bedrock
                .iter()
                .rposition(|w| *w != 0)
                .map_or(0, |i| i + 1);
            chunk_tag = chunk_tag.set(
                "below_zero_retrogen",
                chunk_tag.create_map([
                    (chunk_tag.create_string("target_status"), target_status),
                    (
                        chunk_tag.create_string("missing_bedrock"),
                        chunk_tag
                            .create_long_list(missing_bedrock[..used].iter().map(|w| *w as i64)),
                    ),
                ]),
            );
            chunk_tag = chunk_tag.set("Status", chunk_tag.create_string("empty"));
        }
        chunk_tag = chunk_tag.set("isLightOn", chunk_tag.create_boolean(false));
    }
    chunk_tag
}

fn shift_upgrade_data(upgrade_data: Dynamic) -> Dynamic {
    upgrade_data.update("Indices", |indices| {
        let mut shifted_indices: Vec<(Dynamic, Dynamic)> = Vec::new();
        if let Some(entries) = indices.get_map_values().result() {
            for (index, data) in entries {
                let Some(i) = index
                    .as_string()
                    .result()
                    .and_then(|s| parse_int(s.as_lossy_str()))
                else {
                    continue;
                };
                let key = index.create_string(i.wrapping_add(4).to_string());
                match shifted_indices.iter_mut().find(|(k, _)| *k == key) {
                    Some(entry) => entry.1 = data,
                    None => shifted_indices.push((key, data)),
                }
            }
        }
        let hashes: Vec<i32> = shifted_indices.iter().map(|(k, _)| k.hash_code()).collect();
        let order = mc_core::java_collections::hash_map_order(&hashes, 16);
        let mut slots: Vec<Option<(Dynamic, Dynamic)>> =
            shifted_indices.into_iter().map(Some).collect();
        indices.create_map(order.into_iter().filter_map(|i| slots[i].take()))
    })
}

/// `BitSet.valueOf(bytes).toLongArray()`.
fn bit_set_to_long_array(bytes: &[i8]) -> Vec<i64> {
    let mut words: Vec<i64> = bytes
        .chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0i64, |w, (i, b)| w | (i64::from(*b as u8) << (8 * i)))
        })
        .collect();
    while words.last() == Some(&0) {
        words.pop();
    }
    words
}

fn update_carving_masks(
    chunk_tag: &Dynamic,
    section_count: usize,
    added_sections_below: usize,
) -> Dynamic {
    let carving_masks = chunk_tag.get("CarvingMasks").or_else_empty_map();
    let carving_masks = carving_masks.update_map_values(|(key, value)| {
        let old_values = bit_set_to_long_array(&value.as_byte_buffer());
        let mut new_values = vec![0i64; 64 * section_count];
        let dest = 64 * added_sections_below;
        if dest + old_values.len() > new_values.len() {
            panic!(
                "java.lang.ArrayIndexOutOfBoundsException: arraycopy: last destination index {} out of bounds for long[{}]",
                dest + old_values.len(),
                new_values.len()
            );
        }
        new_values[dest..dest + old_values.len()].copy_from_slice(&old_values);
        (key, chunk_tag.create_long_list(new_values))
    });
    chunk_tag.set("CarvingMasks", carving_masks)
}

fn add_padding_entries(chunk_tag: Dynamic, key: &str) -> Dynamic {
    let mut list = chunk_tag.get(key).or_else_empty_list().as_stream();
    if list.len() == 24 {
        return chunk_tag;
    }
    let empty_list = chunk_tag.empty_list();
    for _ in 0..4 {
        list.insert(0, empty_list.clone());
        list.push(empty_list.clone());
    }
    chunk_tag.set(key, chunk_tag.create_list(list))
}

fn update_heightmaps(chunk_tag: Dynamic) -> Dynamic {
    chunk_tag.update("Heightmaps", |mut heightmap_tag| {
        for heightmap_type in HEIGHTMAP_TYPES {
            heightmap_tag = heightmap_tag.update(heightmap_type, get_fixed_heightmap);
        }
        heightmap_tag
    })
}

fn get_fixed_heightmap(tag: Dynamic) -> Dynamic {
    let values: Vec<i64> = tag
        .as_long_stream()
        .into_iter()
        .map(|value| {
            let mut new_value = 0i64;
            let mut bit_index = 0;
            while bit_index + 9 <= 64 {
                let old_height = value >> bit_index & 511;
                let new_height = if old_height == 0 {
                    0
                } else {
                    (old_height + 64).min(511)
                };
                new_value |= new_height << bit_index;
                bit_index += 9;
            }
            new_value
        })
        .collect();
    tag.create_long_list(values)
}

fn make_biome_container(tag: &Dynamic, source_storage: impl Fn(usize) -> i32) -> Dynamic {
    // `Int2IntLinkedOpenHashMap`: ids in first-seen order
    let mut ids: Vec<i32> = Vec::new();
    for i in 0..64 {
        let biome_id = source_storage(i);
        if !ids.contains(&biome_id) {
            ids.push(biome_id);
        }
    }
    let palette = tag.create_list(ids.iter().map(|id| {
        let name = BIOMES_BY_ID
            .iter()
            .find(|(k, _)| k == id)
            .map_or("minecraft:plains", |(_, v)| v);
        tag.create_string(name)
    }));
    let bits = ceillog2(ids.len() as i32);
    if bits == 0 {
        return make_paletted_container(&palette);
    }
    let values_per_long = 64 / bits;
    let required_length = ((64 + values_per_long - 1) / values_per_long) as usize;
    let mut bit_storage = vec![0i64; required_length];
    let mut cell_index = 0;
    let mut bit_index = 0;
    for i in 0..64 {
        let biome_id = source_storage(i);
        let id = ids
            .iter()
            .position(|k| *k == biome_id)
            .expect("the id was collected above");
        bit_storage[cell_index] |= (id as i64) << bit_index;
        bit_index += bits;
        if bit_index + bits > 64 {
            cell_index += 1;
            bit_index = 0;
        }
    }
    let storage = tag.create_long_list(bit_storage);
    make_paletted_container_with_data(&palette, &storage)
}

fn make_paletted_container(palette: &Dynamic) -> Dynamic {
    palette.create_map([(palette.create_string("palette"), palette.clone())])
}

fn make_paletted_container_with_data(palette: &Dynamic, storage: &Dynamic) -> Dynamic {
    palette.create_map([
        (palette.create_string("palette"), palette.clone()),
        (palette.create_string("data"), storage.clone()),
    ])
}

fn make_optimized_paletted_container(palette: &Dynamic, data: &Dynamic) -> Dynamic {
    let mut palette_list = palette.as_stream();
    if palette_list.len() == 1 {
        return make_paletted_container(palette);
    }
    let palette = pad_palette_entries(palette, data, &mut palette_list);
    make_paletted_container_with_data(&palette, data)
}

fn pad_palette_entries(
    palette: &Dynamic,
    data: &Dynamic,
    palette_list: &mut Vec<Dynamic>,
) -> Dynamic {
    let data_size_in_bits = data.as_long_stream().len() as i64 * 64;
    let estimated_bits_per_block = data_size_in_bits / 4096;
    let palette_size = palette_list.len() as i32;
    let expected_bits_per_block = ceillog2(palette_size);
    if estimated_bits_per_block <= i64::from(expected_bits_per_block) {
        return palette.clone();
    }
    let air_pallete_entry = palette.create_map([(
        palette.create_string("Name"),
        palette.create_string("minecraft:air"),
    )]);
    let minimum_palette_size_to_match_data = 1i32
        .wrapping_shl((estimated_bits_per_block - 1) as u32)
        .wrapping_add(1);
    let additional_palette_entries = minimum_palette_size_to_match_data.wrapping_sub(palette_size);
    for _ in 0..additional_palette_entries {
        palette_list.push(air_pallete_entry.clone());
    }
    palette.create_list(palette_list.clone())
}

/// `ChunkHeightAndBiomeFix.ceillog2`.
pub fn ceillog2(input: i32) -> i32 {
    if input == 0 {
        0
    } else {
        ((f64::from(input)).ln() / 2.0f64.ln()).ceil() as i32
    }
}
