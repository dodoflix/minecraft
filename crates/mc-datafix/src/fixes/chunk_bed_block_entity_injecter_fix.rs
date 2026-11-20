//! `ChunkBedBlockEntityInjecterFix`: adds a `minecraft:bed` block entity (red) for every legacy bed block of a chunk.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::value::Value;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};

/// `new ChunkBedBlockEntityInjecterFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "ChunkBedBlockEntityInjecterFix",
        output_schema,
        changes_type,
        |df| {
            let chunk_type = df.output_schema().get_type(&References::CHUNK);
            let level_type = chunk_type.find_field_type("Level");
            let tile_entity_list_type = level_type.find_field_type("TileEntities");
            let Some(tile_entity_type) = tile_entity_list_type.as_list().cloned() else {
                panic!("java.lang.IllegalStateException: Tile entity type is not a list type.");
            };
            let level_f = DSL::field_finder("Level", &level_type);
            let tile_entities_f = DSL::field_finder("TileEntities", &tile_entity_list_type);
            let input_choice = df
                .input_schema()
                .find_choice_type(&References::BLOCK_ENTITY);
            let output_choice = df
                .output_schema()
                .find_choice_type(&References::BLOCK_ENTITY);
            TypeRewriteRule::seq(
                df.fix_type_everywhere_new(
                    "InjectBedBlockEntityType",
                    input_choice.ty(),
                    output_choice.ty(),
                    |_, v| v,
                ),
                df.fix_type_everywhere_typed(
                    "BedBlockEntityInjecter",
                    &df.output_schema().get_type(&References::CHUNK),
                    move |input| {
                        let level = input.get_typed(&level_f);
                        let level_tag = level.get(&remainder_finder()).into_dynamic();
                        let chunk_x = level_tag.get("xPos").as_int_or(0);
                        let chunk_z = level_tag.get("zPos").as_int_or(0);
                        let mut tile_entities = level.get_or_create(&tile_entities_f).into_list();
                        for section_tag in level_tag.get("Sections").as_list(|d| d) {
                            let pos = section_tag.get("Y").as_int_or(0);
                            for (index, block) in
                                section_tag.get("Blocks").as_int_stream().into_iter().enumerate()
                            {
                                if 416 != (block & 0xFF) << 4 {
                                    continue;
                                }
                                let p = index as i32;
                                let xx = p & 15;
                                let yy = p >> 8 & 15;
                                let zz = p >> 4 & 15;
                                let bed_tag = vec![
                                    (
                                        section_tag.create_string("id"),
                                        section_tag.create_string("minecraft:bed"),
                                    ),
                                    (
                                        section_tag.create_string("x"),
                                        section_tag.create_int(xx.wrapping_add(chunk_x << 4)),
                                    ),
                                    (
                                        section_tag.create_string("y"),
                                        section_tag.create_int(yy.wrapping_add(pos << 4)),
                                    ),
                                    (
                                        section_tag.create_string("z"),
                                        section_tag.create_int(zz.wrapping_add(chunk_z << 4)),
                                    ),
                                    (
                                        section_tag.create_string("color"),
                                        section_tag.create_short(14),
                                    ),
                                ];
                                // `Maps.newHashMap()`: the entries reach `createMap` in `Dynamic.hashCode` bucket order
                                let bed_tag = hash_map_ordered(
                                    bed_tag,
                                    |(k, _)| k.hash_code(),
                                    HASH_MAP_DEFAULT_CAPACITY,
                                );
                                let (bed, _) = tile_entity_type
                                    .read(&section_tag.create_map(bed_tag))
                                    .result()
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "java.lang.IllegalStateException: Could not parse newly created bed block entity."
                                        )
                                    });
                                tile_entities.push(bed);
                            }
                        }
                        if tile_entities.is_empty() {
                            input
                        } else {
                            input.set_typed(
                                &level_f,
                                level.set(&tile_entities_f, Value::List(tile_entities)),
                            )
                        }
                    },
                ),
            )
        },
    )
}
