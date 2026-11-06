//! The `ProtoChunk` / `BiomeResolver` helpers of `levelgen/BelowZeroRetrogen.java` (the persisted state is
//! [`mc_world::below_zero_retrogen::BelowZeroRetrogen`]).

use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_world::below_zero_retrogen::BelowZeroRetrogen;
use mc_world::chunk::ProtoChunk;
use mc_world::palette::BiomeId;

/// `BelowZeroRetrogen.replaceOldBedrock(chunk)`: old bedrock in `y = 0..=4` becomes deepslate.
pub fn replace_old_bedrock(chunk: &mut ProtoChunk) {
    let max_generated_bedrock_y = 4;
    for z in 0..=15 {
        for y in 0..=max_generated_bedrock_y {
            for x in 0..=15 {
                let pos = BlockPos::new(x, y, z);
                if chunk.get_block_state(pos).block() == Block::BEDROCK {
                    chunk.set_block_state(pos, Block::DEEPSLATE.default_state(), None);
                }
            }
        }
    }
}

/// `retrogen.applyBedrockMask(chunk)`: air for every column of the generation height with a missing-bedrock bit.
pub fn apply_bedrock_mask(retrogen: &BelowZeroRetrogen, chunk: &mut ProtoChunk) {
    let height_accessor = chunk.get_height_accessor_for_generation();
    let (min_y, max_y) = (height_accessor.min_y, height_accessor.max_y());
    for x in 0..16 {
        for z in 0..16 {
            if retrogen.has_bedrock_hole(x, z) {
                for y in min_y..=max_y {
                    chunk.set_block_state(BlockPos::new(x, y, z), Block::AIR.default_state(), None);
                }
            }
        }
    }
}

/// `BelowZeroRetrogen.getBiomeResolver(biomeResolver, protoChunk)`: for an upgrading chunk every biome except the
/// retained ones (`is_retained`: `RETAINED_RETROGEN_BIOMES` = lush caves, dripstone caves, deep dark) is replaced by
/// the chunk's own biome at `y = 0`.
///
/// The chunk's `getNoiseBiome(quartX, 0, quartZ)` is read up front (vanilla reads it lazily, but the sections being
/// filled lie below `y = 0`, so the values cannot change meanwhile).
pub fn get_biome_resolver<'a>(
    biome_resolver: Box<dyn Fn(i32, i32, i32) -> BiomeId + 'a>,
    proto_chunk: &ProtoChunk,
    is_retained: Box<dyn Fn(BiomeId) -> bool + 'a>,
) -> Box<dyn Fn(i32, i32, i32) -> BiomeId + 'a> {
    if !proto_chunk.is_upgrading() {
        return biome_resolver;
    }
    let chunk_biomes: [BiomeId; 16] = std::array::from_fn(|i| {
        proto_chunk
            .access
            .get_noise_biome((i & 3) as i32, 0, (i >> 2) as i32)
    });
    Box::new(move |quart_x, quart_y, quart_z| {
        let noise_biome = biome_resolver(quart_x, quart_y, quart_z);
        if is_retained(noise_biome) {
            noise_biome
        } else {
            chunk_biomes[((quart_z & 3) * 4 + (quart_x & 3)) as usize]
        }
    })
}
