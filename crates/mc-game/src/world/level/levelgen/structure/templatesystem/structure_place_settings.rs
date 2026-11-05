//! `StructurePlaceSettings`.

use super::liquid_settings::LiquidSettings;
use super::structure_processor::StructureProcessor;
use super::structure_template::Palette;
use mc_core::bounding_box::BoundingBox;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::pos::BlockPos;
use mc_core::random::{self, PositionalRandomFactory, RandomSource};
use mc_core::rotation::Rotation;
use std::sync::{Arc, Mutex};

/// A random source shared by reference (vanilla stores the `RandomSource` object itself in the settings, so every
/// `getRandom(pos)` consumer advances the same stream).
#[derive(Clone)]
pub struct SharedRandom(Arc<Mutex<Box<dyn RandomSource>>>);

impl SharedRandom {
    pub fn new(random: Box<dyn RandomSource>) -> SharedRandom {
        SharedRandom(Arc::new(Mutex::new(random)))
    }

    fn with<R>(&self, f: impl FnOnce(&mut dyn RandomSource) -> R) -> R {
        f(&mut **self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl RandomSource for SharedRandom {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        self.with(|r| r.fork())
    }
    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        self.with(|r| r.fork_positional())
    }
    fn set_seed(&mut self, seed: i64) {
        self.with(|r| r.set_seed(seed))
    }
    fn next_int(&mut self) -> i32 {
        self.with(|r| r.next_int())
    }
    fn next_int_bounded(&mut self, bound: i32) -> i32 {
        self.with(|r| r.next_int_bounded(bound))
    }
    fn next_long(&mut self) -> i64 {
        self.with(|r| r.next_long())
    }
    fn next_boolean(&mut self) -> bool {
        self.with(|r| r.next_boolean())
    }
    fn next_float(&mut self) -> f32 {
        self.with(|r| r.next_float())
    }
    fn next_double(&mut self) -> f64 {
        self.with(|r| r.next_double())
    }
    fn next_gaussian(&mut self) -> f64 {
        self.with(|r| r.next_gaussian())
    }
    fn legacy_next(&mut self, bits: i32) -> Option<i32> {
        self.with(|r| r.legacy_next(bits))
    }
}

#[derive(Clone)]
pub struct StructurePlaceSettings {
    mirror: Mirror,
    rotation: Rotation,
    rotation_pivot: BlockPos,
    ignore_entities: bool,
    bounding_box: Option<BoundingBox>,
    liquid_settings: LiquidSettings,
    random: Option<SharedRandom>,
    processors: Vec<Arc<dyn StructureProcessor>>,
    known_shape: bool,
    finalize_entities: bool,
}

impl Default for StructurePlaceSettings {
    fn default() -> Self {
        StructurePlaceSettings {
            mirror: Mirror::None,
            rotation: Rotation::None,
            rotation_pivot: BlockPos::ZERO,
            ignore_entities: false,
            bounding_box: None,
            liquid_settings: LiquidSettings::ApplyWaterlogging,
            random: None,
            processors: Vec::new(),
            known_shape: false,
            finalize_entities: false,
        }
    }
}

impl StructurePlaceSettings {
    pub fn new() -> StructurePlaceSettings {
        StructurePlaceSettings::default()
    }

    /// `copy()` is `Clone` (the random source stays shared, like vanilla's reference copy).
    pub fn set_mirror(&mut self, mirror: Mirror) -> &mut Self {
        self.mirror = mirror;
        self
    }

    pub fn set_rotation(&mut self, rotation: Rotation) -> &mut Self {
        self.rotation = rotation;
        self
    }

    pub fn set_rotation_pivot(&mut self, rotation_pivot: BlockPos) -> &mut Self {
        self.rotation_pivot = rotation_pivot;
        self
    }

    pub fn set_ignore_entities(&mut self, ignore_entities: bool) -> &mut Self {
        self.ignore_entities = ignore_entities;
        self
    }

    pub fn set_bounding_box(&mut self, bounding_box: BoundingBox) -> &mut Self {
        self.bounding_box = Some(bounding_box);
        self
    }

    pub fn set_random(&mut self, random: Option<SharedRandom>) -> &mut Self {
        self.random = random;
        self
    }

    pub fn set_liquid_settings(&mut self, liquid_settings: LiquidSettings) -> &mut Self {
        self.liquid_settings = liquid_settings;
        self
    }

    pub fn set_known_shape(&mut self, known_shape: bool) -> &mut Self {
        self.known_shape = known_shape;
        self
    }

    pub fn clear_processors(&mut self) -> &mut Self {
        self.processors.clear();
        self
    }

    pub fn add_processor(&mut self, processor: Arc<dyn StructureProcessor>) -> &mut Self {
        self.processors.push(processor);
        self
    }

    /// `popProcessor(processor)`: removes the first occurrence of that very processor.
    pub fn pop_processor(&mut self, processor: &Arc<dyn StructureProcessor>) -> &mut Self {
        if let Some(i) = self.processors.iter().position(|p| Arc::ptr_eq(p, processor)) {
            self.processors.remove(i);
        }
        self
    }

    pub fn set_finalize_entities(&mut self, finalize_entities: bool) -> &mut Self {
        self.finalize_entities = finalize_entities;
        self
    }

    pub fn get_mirror(&self) -> Mirror {
        self.mirror
    }

    pub fn get_rotation(&self) -> Rotation {
        self.rotation
    }

    pub fn get_rotation_pivot(&self) -> BlockPos {
        self.rotation_pivot
    }

    /// `getRandom(pos)`: the shared random when set, else a fresh one seeded from `pos` (wall clock when `None`).
    pub fn get_random(&self, pos: Option<BlockPos>) -> Box<dyn RandomSource> {
        match (&self.random, pos) {
            (Some(random), _) => Box::new(random.clone()),
            (None, Some(pos)) => Box::new(random::create(mth::get_seed(&pos))),
            (None, None) => Box::new(random::create(
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64),
            )),
        }
    }

    pub fn is_ignore_entities(&self) -> bool {
        self.ignore_entities
    }

    pub fn get_bounding_box(&self) -> Option<BoundingBox> {
        self.bounding_box
    }

    pub fn get_known_shape(&self) -> bool {
        self.known_shape
    }

    pub fn get_processors(&self) -> &[Arc<dyn StructureProcessor>] {
        &self.processors
    }

    pub fn should_apply_waterlogging(&self) -> bool {
        self.liquid_settings == LiquidSettings::ApplyWaterlogging
    }

    /// `getRandomPalette(palettes, pos)`; panics on an empty list (`IllegalStateException("No palettes")`).
    pub fn get_random_palette<'a>(&self, palettes: &'a [Palette], pos: Option<BlockPos>) -> &'a Palette {
        assert!(!palettes.is_empty(), "No palettes");
        &palettes[self.get_random(pos).next_int_bounded(palettes.len() as i32) as usize]
    }

    pub fn should_finalize_entities(&self) -> bool {
        self.finalize_entities
    }
}
