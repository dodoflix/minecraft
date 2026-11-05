//! `LevelChunkSection`: a 16x16x16 block state container, a 4x4x4 biome container and the cached
//! counts (`net.minecraft.world.level.chunk.LevelChunkSection`).

use crate::palette::{BiomeId, PaletteError, PalettedContainer, PalettedContainerFactory};
use mc_data::block::BlockState;

pub const BIOME_CONTAINER_BITS: i32 = 2;

#[derive(Clone, Debug)]
pub struct LevelChunkSection {
    non_empty_block_count: i16,
    fluid_count: i16,
    ticking_block_count: i16,
    ticking_fluid_count: i16,
    states: PalettedContainer<BlockState>,
    biomes: PalettedContainer<BiomeId>,
}

/// Contribution of one block state to the four counters (`non_empty`, `fluid`, `ticking_block`, `ticking_fluid`).
fn state_counts(state: BlockState) -> [i16; 4] {
    if state.is_air() {
        return [0; 4];
    }
    let fluid = state.get_fluid_state();
    let has_fluid = !fluid.is_empty();
    [1, i16::from(has_fluid), i16::from(state.is_randomly_ticking()), i16::from(has_fluid && fluid.data().random_ticking)]
}

impl LevelChunkSection {
    /// `new LevelChunkSection(states, biomes)` (recounts).
    pub fn new(states: PalettedContainer<BlockState>, biomes: PalettedContainer<BiomeId>) -> Self {
        let mut s = LevelChunkSection {
            non_empty_block_count: 0,
            fluid_count: 0,
            ticking_block_count: 0,
            ticking_fluid_count: 0,
            states,
            biomes,
        };
        s.recalc_block_counts();
        s
    }

    /// `new LevelChunkSection(containerFactory)`: all air, default biome.
    pub fn empty(factory: &PalettedContainerFactory) -> Self {
        LevelChunkSection {
            non_empty_block_count: 0,
            fluid_count: 0,
            ticking_block_count: 0,
            ticking_fluid_count: 0,
            states: factory.create_for_block_states(),
            biomes: factory.create_for_biomes(),
        }
    }

    pub fn get_block_state(&self, section_x: i32, section_y: i32, section_z: i32) -> BlockState {
        self.states.get(section_x, section_y, section_z)
    }

    pub fn get_fluid_state(&self, section_x: i32, section_y: i32, section_z: i32) -> mc_data::fluid::FluidState {
        self.get_block_state(section_x, section_y, section_z).get_fluid_state()
    }

    /// `setBlockState`: returns the previous state and updates the counters.
    pub fn set_block_state(&mut self, section_x: i32, section_y: i32, section_z: i32, state: BlockState) -> BlockState {
        let previous = self.states.get_and_set(section_x, section_y, section_z, state);
        let [ne, f, tb, tf] = state_counts(previous);
        self.non_empty_block_count -= ne;
        self.fluid_count -= f;
        self.ticking_block_count -= tb;
        self.ticking_fluid_count -= tf;
        let [ne, f, tb, tf] = state_counts(state);
        self.non_empty_block_count += ne;
        self.fluid_count += f;
        self.ticking_block_count += tb;
        self.ticking_fluid_count += tf;
        previous
    }

    pub fn has_only_air(&self) -> bool {
        self.non_empty_block_count == 0
    }

    pub fn has_fluid(&self) -> bool {
        self.fluid_count > 0
    }

    pub fn is_randomly_ticking(&self) -> bool {
        self.is_randomly_ticking_blocks() || self.is_randomly_ticking_fluids()
    }

    pub fn is_randomly_ticking_blocks(&self) -> bool {
        self.ticking_block_count > 0
    }

    pub fn is_randomly_ticking_fluids(&self) -> bool {
        self.ticking_fluid_count > 0
    }

    pub fn non_empty_block_count(&self) -> i16 {
        self.non_empty_block_count
    }

    pub fn fluid_count(&self) -> i16 {
        self.fluid_count
    }

    pub fn recalc_block_counts(&mut self) {
        let mut totals = [0i32; 4];
        self.states.count(|state, count| {
            for (t, c) in totals.iter_mut().zip(state_counts(state)) {
                *t += i32::from(c) * count as i32;
            }
        });
        self.non_empty_block_count = totals[0] as i16;
        self.fluid_count = totals[1] as i16;
        self.ticking_block_count = totals[2] as i16;
        self.ticking_fluid_count = totals[3] as i16;
    }

    pub fn states(&self) -> &PalettedContainer<BlockState> {
        &self.states
    }

    pub fn biomes(&self) -> &PalettedContainer<BiomeId> {
        &self.biomes
    }

    /// `read(buffer)` (network format).
    pub fn read(&mut self, input: &mut &[u8]) -> Result<(), PaletteError> {
        let take2 = |input: &mut &[u8]| -> Result<i16, PaletteError> {
            let Some((c, rest)) = input.split_first_chunk::<2>() else {
                return Err(PaletteError("Not enough bytes in buffer".into()));
            };
            *input = rest;
            Ok(i16::from_be_bytes(*c))
        };
        self.non_empty_block_count = take2(input)?;
        self.fluid_count = take2(input)?;
        self.states.read(input)?;
        let mut biomes = self.biomes.recreate();
        biomes.read(input)?;
        self.biomes = biomes;
        Ok(())
    }

    /// `readBiomes(buffer)`.
    pub fn read_biomes(&mut self, input: &mut &[u8]) -> Result<(), PaletteError> {
        let mut biomes = self.biomes.recreate();
        biomes.read(input)?;
        self.biomes = biomes;
        Ok(())
    }

    /// `write(buffer)` (network format): the two counts, block states, biomes.
    pub fn write(&self, out: &mut Vec<u8>) -> Result<(), PaletteError> {
        out.extend_from_slice(&self.non_empty_block_count.to_be_bytes());
        out.extend_from_slice(&self.fluid_count.to_be_bytes());
        self.states.write(out)?;
        self.biomes.write(out)
    }

    pub fn serialized_size(&self) -> Result<usize, PaletteError> {
        Ok(4 + self.states.serialized_size()? + self.biomes.serialized_size()?)
    }

    pub fn maybe_has(&self, pred: impl FnMut(BlockState) -> bool) -> bool {
        self.states.maybe_has(pred)
    }

    pub fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.biomes.get(quart_x, quart_y, quart_z)
    }

    /// `fillBiomesFromNoise`: x outer, y, z inner, exactly vanilla's call order.
    pub fn fill_biomes_from_noise(
        &mut self,
        mut resolver: impl FnMut(i32, i32, i32) -> BiomeId,
        quart_min_x: i32,
        quart_min_y: i32,
        quart_min_z: i32,
    ) {
        let mut new_biomes = self.biomes.recreate();
        for x in 0..4 {
            for y in 0..4 {
                for z in 0..4 {
                    new_biomes.get_and_set(x, y, z, resolver(quart_min_x + x, quart_min_y + y, quart_min_z + z));
                }
            }
        }
        self.biomes = new_biomes;
    }

    /// [`Self::fill_biomes_from_noise`] for a resolver that reads the biome currently stored at the queried cell
    /// (`chunk.getNoiseBiome(..)` inside `/fillbiome`'s resolver: the old container is replaced only afterwards).
    pub fn fill_biomes_from_noise_with_current(
        &mut self,
        mut resolver: impl FnMut(i32, i32, i32, BiomeId) -> BiomeId,
        quart_min_x: i32,
        quart_min_y: i32,
        quart_min_z: i32,
    ) {
        let mut new_biomes = self.biomes.recreate();
        for x in 0..4 {
            for y in 0..4 {
                for z in 0..4 {
                    let current = self.biomes.get(x, y, z);
                    new_biomes.get_and_set(x, y, z, resolver(quart_min_x + x, quart_min_y + y, quart_min_z + z, current));
                }
            }
        }
        self.biomes = new_biomes;
    }
}
