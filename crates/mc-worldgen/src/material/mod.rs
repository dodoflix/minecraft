//! `net.minecraft.world.level.levelgen.material`: the data-driven material rule / condition system
//! that replaced `SurfaceRules`, ore vein placement and bedrock layers, together with the
//! [`MaterialSystem`] that applies a rule to a chunk's columns.
//!
//! `MaterialRule` / `MaterialCondition` decode from `data/<ns>/worldgen/material_rule|material_condition`
//! JSON: a string is a holder reference (decoded once per registry entry and shared), an object is inline.
//! Reference cycles are an error. `biome_is` sets and temperature checks need the world's
//! [`BiomeRegistry`](crate::biome::BiomeRegistry); decoded rules are only meaningful together with the
//! registry they were decoded with (the registry memoizes entries per [`WorldgenRegistries`]).

mod cave_surface;
mod condition;
mod context;
mod rule;
mod system;

use std::cell::RefCell;
use std::sync::Arc;

use mc_core::ident::Identifier;
use serde_json::Value;

pub use cave_surface::CaveSurface;
pub use condition::MaterialCondition;
pub use context::MaterialRuleContext;
pub use rule::{MaterialRule, OreVeinRule};
pub use system::MaterialSystem;

use crate::biome::BiomeRegistry;
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{LoadResult, parse_identifier};

/// `Registries.MATERIAL_RULE` folder.
pub const MATERIAL_RULE_REGISTRY: &str = "worldgen/material_rule";
/// `Registries.MATERIAL_CONDITION` folder.
pub const MATERIAL_CONDITION_REGISTRY: &str = "worldgen/material_condition";

/// What decoding a rule or condition needs: the registries to resolve references against.
pub(crate) struct Decoder<'a> {
    pub(crate) registries: &'a WorldgenRegistries,
    pub(crate) biomes: &'a BiomeRegistry,
}

thread_local! {
    /// Registry entries currently being decoded on this thread (reference cycle detection).
    static LOADING: RefCell<Vec<(&'static str, Identifier)>> = const { RefCell::new(Vec::new()) };
}

impl Decoder<'_> {
    /// Resolves the reference `key` in `registry`: decoded once (with `decode`) and shared like a
    /// `Holder.Reference`. A reference cycle is an error.
    pub(crate) fn reference<T: Send + Sync + 'static>(
        &self,
        registry: &'static str,
        key: &Identifier,
        decode: impl FnOnce(&Value) -> LoadResult<T>,
    ) -> LoadResult<Arc<T>> {
        let cyclic = LOADING.with(|loading| {
            loading.borrow().iter().any(|(r, k)| *r == registry && k == key)
        });
        if cyclic {
            return Err(LoadError::new("reference cycle").in_entry(registry, key));
        }
        LOADING.with(|loading| loading.borrow_mut().push((registry, key.clone())));
        let result = self.registries.memoized(registry, key, |json, _| decode(json));
        LOADING.with(|loading| loading.borrow_mut().pop());
        result
    }
}

/// The `type` of a dispatched codec: an identifier in the `minecraft` namespace; returns its path.
pub(crate) fn parse_type(value: &Value) -> LoadResult<String> {
    let id = parse_identifier(value)?;
    if id.namespace() == "minecraft" {
        Ok(id.path().to_owned())
    } else {
        Err(LoadError::new(format!("Unknown registry key: {id}")))
    }
}

/// `ResourceKey.codec(Registries.NOISE)`. Unlike vanilla (which fails when the rule is first compiled) the
/// noise must exist at decode time.
pub(crate) fn parse_noise_key(value: &Value, decoder: &Decoder) -> LoadResult<Identifier> {
    let id = parse_identifier(value)?;
    decoder.registries.noise(&id)?;
    Ok(id)
}
