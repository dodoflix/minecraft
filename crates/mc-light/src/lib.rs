//! Light engine (sky + block) over mc-world chunks: port of `net.minecraft.world.level.lighting.**` and the light
//! parts of `ThreadedLevelLightEngine`. `ChunkSkyLightSources` lives in `mc_world::sky_light_sources` because every
//! chunk owns one.

pub mod chunk_source;
pub mod engine;
pub mod level_light_engine;
pub mod long_set;
pub mod storage;
pub mod threaded;

pub use chunk_source::{ChunkMapGetter, LightChunk, LightChunkGetter};
pub use engine::{LightEngine, MAX_LEVEL, get_light_dampening_into, has_different_light_properties};
pub use level_light_engine::{LIGHT_SECTION_PADDING, LevelLightEngine, LightReader};
pub use mc_world::sky_light_sources::{ChunkSkyLightSources, get_occlusion_shape, is_empty_shape};
pub use storage::{LightLayer, SectionType};
pub use threaded::{LightCompletion, ThreadedLevelLightEngine, UpdateResult};
