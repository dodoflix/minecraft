//! `BitStorageAlignFix`: repacks chunk section block states and heightmaps so that entries no longer straddle two longs.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_core::math::mth::{ceillog2, is_power_of_two_i32};
use mc_dfu::Dynamic;

const HEIGHTMAP_BITS: i32 = 9;
const HEIGHTMAP_SIZE: i32 = 256;

/// `new BitStorageAlignFix(schema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("BitStorageAlignFix", schema, false, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_type = chunk_type.find_field_type("Level");
        let level_finder = DSL::field_finder("Level", &level_type);
        let sections_finder = level_finder.ty().find_field("Sections");
        let section_type = sections_finder
            .ty()
            .as_list()
            .expect("java.lang.ClassCastException: Sections is not a list type")
            .clone();
        let section_finder = DSL::type_finder(&section_type);
        let block_state_type =
            DSL::named_type(References::BLOCK_STATE.type_name(), &DSL::remainder_type());
        let palette_finder = DSL::field_finder("Palette", &DSL::list_type(&block_state_type));
        df.fix_type_everywhere_typed_new(
            "BitStorageAlignFix",
            &chunk_type,
            &df.output_schema().get_type(&References::CHUNK),
            move |chunk| {
                chunk.update_typed(&level_finder, |level| {
                    update_heightmaps(update_sections(
                        &sections_finder,
                        &section_finder,
                        &palette_finder,
                        level,
                    ))
                })
            },
        )
    })
}

fn update_heightmaps(level: Typed) -> Typed {
    level.update_remainder(|tag| {
        tag.update("Heightmaps", |heightmaps| {
            heightmaps.update_map_values(|(key, heightmap)| {
                (
                    key,
                    update_bit_storage(&tag, &heightmap, HEIGHTMAP_SIZE, HEIGHTMAP_BITS),
                )
            })
        })
    })
}

fn update_sections(
    sections_finder: &OpticFinder,
    section_finder: &OpticFinder,
    palette_finder: &OpticFinder,
    level: Typed,
) -> Typed {
    level.update_typed(sections_finder, |sections| {
        sections.update_typed(section_finder, |section| {
            let bits = section
                .get_optional(palette_finder)
                .map_or(0, |palette| ceillog2(palette.as_list().len() as i32).max(4));
            if bits != 0 && !is_power_of_two_i32(bits) {
                section.update_remainder(|tag| {
                    tag.update("BlockStates", |states| {
                        update_bit_storage(&tag, &states, 4096, bits)
                    })
                })
            } else {
                section
            }
        })
    })
}

fn update_bit_storage(tag: &Dynamic, storage: &Dynamic, size: i32, bits: i32) -> Dynamic {
    let input = storage.as_long_stream();
    tag.create_long_list(add_padding(size, bits, &input))
}

/// `BitStorageAlignFix.addPadding(size, bits, data)`.
pub fn add_padding(size: i32, bits: i32, data: &[i64]) -> Vec<i64> {
    let data_length = data.len() as i32;
    if data_length == 0 {
        return data.to_vec();
    }
    // Java shifts mask the distance to 6 bits; `wrapping_sh*` does the same.
    let mask: i64 = 1i64.wrapping_shl(bits as u32).wrapping_sub(1);
    let values_per_long = 64 / bits;
    let required_length = (size + values_per_long - 1) / values_per_long;
    let mut result = vec![0i64; required_length as usize];
    let mut output_data_index = 0usize;
    let mut output_start = 0i32;
    let mut output_data = 0i64;
    let mut current_index = 0i32;
    let mut current = data[0];
    let mut next = if data_length > 1 { data[1] } else { 0 };

    for index in 0..size {
        let position = index * bits;
        let start_data = position >> 6;
        let end_data = ((index + 1) * bits - 1) >> 6;
        let start_bit = position ^ (start_data << 6);
        if start_data != current_index {
            current = next;
            next = if start_data + 1 < data_length {
                data[(start_data + 1) as usize]
            } else {
                0
            };
            current_index = start_data;
        }

        let value_to_insert = if start_data == end_data {
            ((current as u64).wrapping_shr(start_bit as u32) as i64) & mask
        } else {
            let shift_bits = 64 - start_bit;
            ((current as u64).wrapping_shr(start_bit as u32) as i64
                | next.wrapping_shl(shift_bits as u32))
                & mask
        };

        let output_end = output_start + bits;
        if output_end >= 64 {
            result[output_data_index] = output_data;
            output_data_index += 1;
            output_data = value_to_insert;
            output_start = bits;
        } else {
            output_data |= value_to_insert.wrapping_shl(output_start as u32);
            output_start = output_end;
        }
    }

    if output_data != 0 {
        result[output_data_index] = output_data;
    }
    result
}
