//! `ChunkBiomeFix`: expands the 256-entry 2D `Level.Biomes` array of a chunk to the 1024-entry 3D layout.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new ChunkBiomeFix(outputSchema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ChunkBiomeFix", schema, changes_type, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_f = chunk_type.find_field("Level");
        df.fix_type_everywhere_typed("Leaves fix", &chunk_type, move |chunk| {
            chunk.update_typed(&level_f, |level| level.update_remainder(fix_biomes))
        })
    })
}

fn fix_biomes(tag: Dynamic) -> Dynamic {
    let Some(old_biomes) = tag.get("Biomes").as_int_stream_opt().result() else {
        return tag;
    };
    if old_biomes.len() != 256 {
        return tag;
    }
    let mut new_biomes = [0i32; 1024];
    for z in 0..4 {
        for x in 0..4 {
            let old_x = (x << 2) + 2;
            let old_z = (z << 2) + 2;
            let index = old_z << 4 | old_x;
            new_biomes[z << 2 | x] = old_biomes[index];
        }
    }
    for y_slice in 1..64 {
        new_biomes.copy_within(0..16, y_slice * 16);
    }
    tag.set("Biomes", tag.create_int_list(new_biomes))
}
