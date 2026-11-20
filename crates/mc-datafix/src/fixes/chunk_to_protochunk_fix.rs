//! `ChunkToProtochunkFix`: converts a legacy chunk into a proto chunk: status, 1D biome bytes to an int array and the
//! pending tile ticks to per-section packed offsets.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

const NUM_SECTIONS: usize = 16;

/// `new ChunkToProtochunkFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ChunkToProtochunkFix", output_schema, changes_type, |df| {
        df.write_fix_and_read(
            "ChunkToProtoChunkFix",
            &df.input_schema().get_type(&References::CHUNK),
            &df.output_schema().get_type(&References::CHUNK),
            |chunk| chunk.update("Level", fix_chunk_data),
        )
    })
}

/// `fixChunkData(tag)`.
fn fix_chunk_data(tag: Dynamic) -> Dynamic {
    let terrain_populated = tag.get("TerrainPopulated").as_boolean_or(false);
    let light_populated = tag.get("LightPopulated").as_number().result().is_none()
        || tag.get("LightPopulated").as_boolean_or(false);
    let status = if terrain_populated {
        if light_populated {
            "mobs_spawned"
        } else {
            "decorated"
        }
    } else {
        "carved"
    };
    repack_ticks(repack_biomes(&tag))
        .set("Status", tag.create_string(status))
        .set("hasLegacyStructureData", tag.create_boolean(true))
}

/// `repackBiomes(tag)`.
fn repack_biomes(tag: &Dynamic) -> Dynamic {
    tag.update("Biomes", |biomes| {
        match biomes.as_byte_buffer_opt().result() {
            Some(buffer) => {
                let mut new_biomes = [0i32; 256];
                for (i, biome) in new_biomes.iter_mut().enumerate() {
                    if i < buffer.len() {
                        *biome = i32::from(buffer[i]) & 255;
                    }
                }
                tag.create_int_list(new_biomes)
            }
            None => biomes,
        }
    })
}

/// `repackTicks(tag)`.
fn repack_ticks(tag: Dynamic) -> Dynamic {
    let Some(ticks) = tag.get("TileTicks").as_stream_opt().result() else {
        return tag;
    };
    let mut to_be_ticked: Vec<Vec<i16>> = vec![Vec::new(); NUM_SECTIONS];
    for pending_tick_tag in &ticks {
        let x = pending_tick_tag.get("x").as_int_or(0);
        let y = pending_tick_tag.get("y").as_int_or(0);
        let z = pending_tick_tag.get("z").as_int_or(0);
        let packed_offset = pack_offset_coordinates(x, y, z);
        match usize::try_from(y >> 4).ok().filter(|&i| i < NUM_SECTIONS) {
            Some(i) => to_be_ticked[i].push(packed_offset),
            None => panic!(
                "java.lang.IndexOutOfBoundsException: Index {} out of bounds for length {NUM_SECTIONS}",
                y >> 4
            ),
        }
    }
    tag.remove("TileTicks").set(
        "ToBeTicked",
        tag.create_list(
            to_be_ticked
                .into_iter()
                .map(|l| tag.create_list(l.into_iter().map(|v| tag.create_short(v)))),
        ),
    )
}

/// `packOffsetCoordinates(x, y, z)`.
fn pack_offset_coordinates(x: i32, y: i32, z: i32) -> i16 {
    (x & 15 | (y & 15) << 4 | (z & 15) << 8) as i16
}
