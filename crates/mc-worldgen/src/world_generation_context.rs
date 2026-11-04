//! `net.minecraft.world.level.levelgen.WorldGenerationContext`.

use mc_world::chunk::LevelHeight;

/// The vertical generation extent of a chunk generator clamped to a height accessor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldGenerationContext {
    min_y: i32,
    height: i32,
    sea_level: i32,
}

impl WorldGenerationContext {
    /// `new WorldGenerationContext(generator, heightAccessor)`: `generator_*` are the generator's
    /// `getMinY()`, `getGenDepth()` and `getSeaLevel()`.
    pub fn new(
        generator_min_y: i32,
        generator_gen_depth: i32,
        generator_sea_level: i32,
        height_accessor: LevelHeight,
    ) -> WorldGenerationContext {
        WorldGenerationContext {
            min_y: height_accessor.min_y.max(generator_min_y),
            height: height_accessor.height.min(generator_gen_depth),
            sea_level: generator_sea_level,
        }
    }

    /// The private `WorldGenerationContext(LevelAccessor)` constructor (plain level extent).
    pub fn of_level(min_y: i32, height: i32, sea_level: i32) -> WorldGenerationContext {
        WorldGenerationContext { min_y, height, sea_level }
    }

    pub fn get_min_gen_y(&self) -> i32 {
        self.min_y
    }

    pub fn get_gen_depth(&self) -> i32 {
        self.height
    }

    pub fn sea_level(&self) -> i32 {
        self.sea_level
    }
}
