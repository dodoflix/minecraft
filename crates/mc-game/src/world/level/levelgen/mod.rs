//! `net.minecraft.world.level.levelgen` gameplay side: features, placement, block predicates/providers, structures and
//! the generation hooks that plug them into the chunk map (`server::level::world_gen::WorldGenHooks`).
//!
//! Layout (each area owns its directory; types register themselves in linkme tables, no central lists):
//! * [`codec`], [`holder`], [`registries`], [`env`]: shared decoding infrastructure and the per-level [`env::GenEnv`].
//! * [`world_gen_level`] / `gen_level`: the `WorldGenLevel` interface and its implementation over the lent chunks.
//! * [`feature`], [`placement`], [`blockpredicates`], [`stateproviders`]: the feature layer.
//! * `decoration`: `applyBiomeDecoration` + `FeatureSorter`; `hooks`: the `WorldGenHooks` implementation.
//! * [`structure`]: structure sets/starts/pieces/templates/jigsaw.

pub mod blockpredicates;
pub mod codec;
pub mod column;
pub mod decoration;
pub mod env;
pub mod feature;
pub mod gen_level;
pub mod generation_step;
pub mod holder;
pub mod hooks;
pub mod placement;
pub mod registries;
pub mod stateproviders;
pub mod structure;
pub mod world_gen_level;

pub use env::GenEnv;
pub use holder::Holder;
pub use world_gen_level::WorldGenLevel;
