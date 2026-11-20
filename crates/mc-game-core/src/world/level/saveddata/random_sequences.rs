//! `RandomSequences` (`data/minecraft/random_sequences.dat`) and `RandomSequence`.

use std::collections::HashMap;

use mc_core::ident::Identifier;
use mc_core::random::{
    PositionalRandomFactory, RandomSource, Xoroshiro128PlusPlus, XoroshiroRandomSource,
    seed_from_hash_of, upgrade_seed_to_128bit_unmixed,
};
use mc_dfu::extra::{IDENTIFIER, fixed_size_longs};
use mc_dfu::{BOOL, Codec, INT, LONG_STREAM};

use super::SavedDataType;
use crate::impl_saved_data;

/// `RandomSequence`: one Xoroshiro stream.
#[derive(Clone, Debug)]
pub struct RandomSequence {
    source: XoroshiroRandomSource,
}

impl RandomSequence {
    pub fn new(seed: i64, key: Option<&Identifier>) -> Self {
        let mut seed128 = upgrade_seed_to_128bit_unmixed(seed);
        if let Some(key) = key {
            seed128 = seed128.xor_seed(Self::seed_for_key(key));
        }
        RandomSequence {
            source: XoroshiroRandomSource::from_seed128(seed128.mixed()),
        }
    }

    /// `seedForKey`.
    pub fn seed_for_key(key: &Identifier) -> mc_core::random::Seed128bit {
        seed_from_hash_of(&key.to_string())
    }

    pub fn random(&mut self) -> &mut XoroshiroRandomSource {
        &mut self.source
    }

    /// `RandomSequence.CODEC`: `{source: [L; lo, hi]}` (a long stream of two).
    pub fn codec() -> Codec<RandomSequence> {
        let source = LONG_STREAM.comap_flat_map(
            |longs| fixed_size_longs(longs, 2).map(|l| [l[0], l[1]]),
            |longs: &[i64; 2]| longs.to_vec(),
        );
        mc_dfu::record!(PackedSequence {
            source: source.field_of("source"),
        })
        .codec()
        .xmap(
            |p| RandomSequence {
                source: XoroshiroRandomSource::from_generator(Xoroshiro128PlusPlus::new(
                    p.source[0],
                    p.source[1],
                )),
            },
            |s: &RandomSequence| PackedSequence {
                source: s.source.generator().to_longs(),
            },
        )
    }
}

/// The serialized shape of [`RandomSequence`].
struct PackedSequence {
    source: [i64; 2],
}

#[derive(Debug, Clone)]
pub struct RandomSequences {
    dirty: bool,
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: HashMap<Identifier, RandomSequence>,
}

impl_saved_data!(RandomSequences);

impl Default for RandomSequences {
    fn default() -> Self {
        RandomSequences {
            dirty: false,
            salt: 0,
            include_world_seed: true,
            include_sequence_id: true,
            sequences: HashMap::new(),
        }
    }
}

/// `RandomSequences.DirtyMarkingRandomSource`: every draw marks the owning saved data dirty.
pub struct DirtyMarkingRandomSource<'a> {
    owner: &'a mut bool,
    random: &'a mut XoroshiroRandomSource,
}

macro_rules! marking {
    ($self:ident, $call:expr) => {{
        *$self.owner = true;
        $call
    }};
}

impl RandomSource for DirtyMarkingRandomSource<'_> {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        marking!(self, self.random.fork())
    }
    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        marking!(self, self.random.fork_positional())
    }
    fn set_seed(&mut self, seed: i64) {
        marking!(self, self.random.set_seed(seed))
    }
    fn next_int(&mut self) -> i32 {
        marking!(self, self.random.next_int())
    }
    fn next_int_bounded(&mut self, bound: i32) -> i32 {
        marking!(self, self.random.next_int_bounded(bound))
    }
    fn next_long(&mut self) -> i64 {
        marking!(self, self.random.next_long())
    }
    fn next_boolean(&mut self) -> bool {
        marking!(self, self.random.next_boolean())
    }
    fn next_float(&mut self) -> f32 {
        marking!(self, self.random.next_float())
    }
    fn next_double(&mut self) -> f64 {
        marking!(self, self.random.next_double())
    }
    fn next_gaussian(&mut self) -> f64 {
        marking!(self, self.random.next_gaussian())
    }
}

impl RandomSequences {
    /// `get(key, worldSeed)`: creates the sequence on first use (which, like vanilla's `computeIfAbsent`, does not by
    /// itself mark the data dirty — only drawing does).
    pub fn get(&mut self, key: &Identifier, world_seed: i64) -> DirtyMarkingRandomSource<'_> {
        if !self.sequences.contains_key(key) {
            let seq = self.create_sequence(
                key,
                world_seed,
                self.salt,
                self.include_world_seed,
                self.include_sequence_id,
            );
            self.sequences.insert(key.clone(), seq);
        }
        DirtyMarkingRandomSource {
            owner: &mut self.dirty,
            random: self.sequences.get_mut(key).expect("present").random(),
        }
    }

    fn create_sequence(
        &self,
        key: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> RandomSequence {
        let seed = (if include_world_seed { world_seed } else { 0 }) ^ (salt as i64);
        RandomSequence::new(seed, include_sequence_id.then_some(key))
    }

    pub fn for_all_sequences(&self, mut consumer: impl FnMut(&Identifier, &RandomSequence)) {
        for (k, v) in &self.sequences {
            consumer(k, v);
        }
    }

    pub fn set_seed_defaults(
        &mut self,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        self.salt = salt;
        self.include_world_seed = include_world_seed;
        self.include_sequence_id = include_sequence_id;
    }

    pub fn clear(&mut self) -> i32 {
        let count = self.sequences.len() as i32;
        self.sequences.clear();
        count
    }

    pub fn reset(&mut self, id: &Identifier, world_seed: i64) {
        let seq = self.create_sequence(
            id,
            world_seed,
            self.salt,
            self.include_world_seed,
            self.include_sequence_id,
        );
        self.sequences.insert(id.clone(), seq);
    }

    pub fn reset_with(
        &mut self,
        id: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        let seq = self.create_sequence(
            id,
            world_seed,
            salt,
            include_world_seed,
            include_sequence_id,
        );
        self.sequences.insert(id.clone(), seq);
    }

    pub fn salt(&self) -> i32 {
        self.salt
    }
    pub fn include_world_seed(&self) -> bool {
        self.include_world_seed
    }
    pub fn include_sequence_id(&self) -> bool {
        self.include_sequence_id
    }

    /// `RandomSequences.CODEC`.
    pub fn codec() -> Codec<RandomSequences> {
        mc_dfu::record!(Packed {
            salt: INT.field_of("salt"),
            include_world_seed: BOOL.optional_field_of_default("include_world_seed", true),
            include_sequence_id: BOOL.optional_field_of_default("include_sequence_id", true),
            sequences: IDENTIFIER
                .unbounded_map(&RandomSequence::codec())
                .field_of("sequences"),
        })
        .codec()
        .xmap(
            |p| RandomSequences {
                dirty: false,
                salt: p.salt,
                include_world_seed: p.include_world_seed,
                include_sequence_id: p.include_sequence_id,
                sequences: p.sequences.into_iter().collect(),
            },
            |s: &RandomSequences| Packed {
                salt: s.salt,
                include_world_seed: s.include_world_seed,
                include_sequence_id: s.include_sequence_id,
                sequences: s
                    .sequences
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            },
        )
    }

    pub fn type_() -> SavedDataType<RandomSequences> {
        SavedDataType::from_codec(
            Identifier::parse("minecraft:random_sequences").expect("static id"),
            RandomSequences::default,
            RandomSequences::codec(),
            mc_datafix::DataFixTypes::SAVED_DATA_RANDOM_SEQUENCES,
        )
    }
}

/// The serialized shape of [`RandomSequences`].
struct Packed {
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: Vec<(Identifier, RandomSequence)>,
}
