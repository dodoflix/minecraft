//! Loading of vanilla worldgen data (`data/<namespace>/<registry>/<path>.json`) into the Rust types.
//!
//! The data source is injectable ([`WorldgenDataSource`]); [`WorldgenRegistries`] memoizes the registries
//! built on top of it so that every reference to an id shares one object, like vanilla's `Holder.Reference`s.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, ThreadId};

use serde_json::Value;

use mc_core::ident::Identifier;

use crate::densityfunction::codec::{
    Fields, LoadResult, density_function_direct_from_json, parse_bool, parse_double_range,
    parse_int_range, parse_list,
};
use crate::densityfunction::{DensityFunctionHolder, NoiseHolder};
use crate::noise_settings::NoiseGeneratorSettings;
use crate::synth::NormalNoise;
use crate::synth::normal_noise::{NoiseParameters, Normalization};

pub const NOISE_REGISTRY: &str = "worldgen/noise";
pub const DENSITY_FUNCTION_REGISTRY: &str = "worldgen/density_function";
pub const NOISE_SETTINGS_REGISTRY: &str = "worldgen/noise_settings";

/// Failure to load or decode worldgen data. `Display`: `<entry>: <field path>: <message>`, e.g.
/// `worldgen/density_function/minecraft:overworld/offset: spline.points[3].value: expected number or object`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    entry: Option<String>,
    path: String,
    message: String,
}

impl LoadError {
    pub fn new(message: impl Into<String>) -> LoadError {
        LoadError {
            entry: None,
            path: String::new(),
            message: message.into(),
        }
    }

    /// Prefixes the field path with the object field `name`.
    pub fn at(mut self, name: &str) -> LoadError {
        self.path = if self.path.is_empty() || self.path.starts_with('[') {
            format!("{name}{}", self.path)
        } else {
            format!("{name}.{}", self.path)
        };
        self
    }

    /// Prefixes the field path with the list index `index`.
    pub fn at_index(mut self, index: usize) -> LoadError {
        self.path = if self.path.is_empty() || self.path.starts_with('[') {
            format!("[{index}]{}", self.path)
        } else {
            format!("[{index}].{}", self.path)
        };
        self
    }

    /// Names the registry entry being decoded; the innermost (first set) entry wins.
    pub fn in_entry(mut self, registry: &str, id: &Identifier) -> LoadError {
        self.entry.get_or_insert_with(|| format!("{registry}/{id}"));
        self
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(entry) = &self.entry {
            write!(f, "{entry}: ")?;
        }
        if !self.path.is_empty() {
            write!(f, "{}: ", self.path)?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for LoadError {}

/// Where raw worldgen JSON comes from. `registry` is the registry folder, e.g. `"worldgen/noise"`.
/// `None` means the entry does not exist (or could not be read/parsed).
pub trait WorldgenDataSource: Send + Sync {
    fn get(&self, registry: &str, id: &Identifier) -> Option<Value>;
}

/// Reads `<root>/<namespace>/<registry>/<path>.json` (`root` is a `data/` directory such as `ref/src/data`).
pub struct FsDataSource {
    root: PathBuf,
}

impl FsDataSource {
    pub fn new(root: impl Into<PathBuf>) -> FsDataSource {
        FsDataSource { root: root.into() }
    }
}

impl WorldgenDataSource for FsDataSource {
    fn get(&self, registry: &str, id: &Identifier) -> Option<Value> {
        // Identifier paths may contain `.` characters, so keep ids from escaping the root.
        if id
            .path()
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        {
            return None;
        }
        let file = self
            .root
            .join(id.namespace())
            .join(registry)
            .join(format!("{}.json", id.path()));
        serde_json::from_slice(&std::fs::read(file).ok()?).ok()
    }
}

/// A [`WorldgenDataSource`] backed by a closure (tests, in-memory data packs).
pub struct ClosureDataSource<F>(F);

impl<F: Fn(&str, &Identifier) -> Option<Value> + Send + Sync> ClosureDataSource<F> {
    pub fn new(get: F) -> ClosureDataSource<F> {
        ClosureDataSource(get)
    }
}

impl<F: Fn(&str, &Identifier) -> Option<Value> + Send + Sync> WorldgenDataSource
    for ClosureDataSource<F>
{
    fn get(&self, registry: &str, id: &Identifier) -> Option<Value> {
        (self.0)(registry, id)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The worldgen registries (noises, density functions, noise settings) loaded lazily from a data source.
pub struct WorldgenRegistries {
    source: Arc<dyn WorldgenDataSource>,
    noises: Mutex<HashMap<Identifier, NoiseHolder>>,
    density_functions: Mutex<HashMap<Identifier, DensityFunctionHolder>>,
    /// Density functions currently being decoded, per thread: a repeat is a reference cycle.
    loading: Mutex<HashSet<(ThreadId, Identifier)>>,
}

impl WorldgenRegistries {
    pub fn new(source: Arc<dyn WorldgenDataSource>) -> WorldgenRegistries {
        WorldgenRegistries {
            source,
            noises: Mutex::default(),
            density_functions: Mutex::default(),
            loading: Mutex::default(),
        }
    }

    fn fetch(&self, registry: &str, id: &Identifier) -> LoadResult<Value> {
        self.source
            .get(registry, id)
            .ok_or_else(|| LoadError::new("no such registry entry").in_entry(registry, id))
    }

    /// The registered `NormalNoise` `id` (`NormalNoise.CODEC` reference).
    pub fn noise(&self, id: &Identifier) -> LoadResult<NoiseHolder> {
        if let Some(holder) = lock(&self.noises).get(id) {
            return Ok(holder.clone());
        }
        let json = self.fetch(NOISE_REGISTRY, id)?;
        let parameters =
            parse_noise_parameters(&json).map_err(|e| e.in_entry(NOISE_REGISTRY, id))?;
        let holder = NoiseHolder {
            key: id.clone(),
            value: Arc::new(NormalNoise::new(parameters)),
        };
        Ok(lock(&self.noises)
            .entry(id.clone())
            .or_insert(holder)
            .clone())
    }

    /// The registered density function `id`; every call for one id returns the same `DensityFunction`.
    pub fn density_function(&self, id: &Identifier) -> LoadResult<DensityFunctionHolder> {
        if let Some(holder) = lock(&self.density_functions).get(id) {
            return Ok(holder.clone());
        }
        let key = (thread::current().id(), id.clone());
        if !lock(&self.loading).insert(key.clone()) {
            return Err(LoadError::new("reference cycle").in_entry(DENSITY_FUNCTION_REGISTRY, id));
        }
        let decoded = self.fetch(DENSITY_FUNCTION_REGISTRY, id).and_then(|json| {
            density_function_direct_from_json(&json, self)
                .map_err(|e| e.in_entry(DENSITY_FUNCTION_REGISTRY, id))
        });
        lock(&self.loading).remove(&key);
        let holder = DensityFunctionHolder {
            key: id.clone(),
            value: decoded?,
        };
        Ok(lock(&self.density_functions)
            .entry(id.clone())
            .or_insert(holder)
            .clone())
    }

    /// The registered `NoiseGeneratorSettings` `id`.
    pub fn noise_settings(&self, id: &Identifier) -> LoadResult<NoiseGeneratorSettings> {
        let json = self.fetch(NOISE_SETTINGS_REGISTRY, id)?;
        NoiseGeneratorSettings::from_json(&json, self)
            .map_err(|e| e.in_entry(NOISE_SETTINGS_REGISTRY, id))
    }
}

/// `NormalNoise.Parameters.CODEC` (with its `validate`).
fn parse_noise_parameters(value: &Value) -> LoadResult<NoiseParameters> {
    let fields = Fields::new(value)?;
    let base_amplitude = fields
        .optional("base_amplitude", |v| {
            parse_double_range(v, f64::from(1.0E-5f32), 1000000.0)
        })?
        .unwrap_or(1.0);
    let base_octave = fields.required("base_octave", |v| parse_int_range(v, -32, 32))?;
    let octave_count = fields
        .optional("octave_count", |v| parse_int_range(v, 1, 32))?
        .unwrap_or(1);
    let normalize = fields
        .optional("normalize", parse_normalization)?
        .unwrap_or(Normalization::Enabled);
    let amplitude_modifiers = fields
        .optional("amplitude_modifiers", |v| {
            let modifiers = parse_list(v, |e| parse_double_range(e, 0.0, 1000000.0))?;
            if modifiers.len() > 32 {
                return Err(LoadError::new(format!(
                    "List is too long: {}, expected at most 32",
                    modifiers.len()
                )));
            }
            Ok(modifiers)
        })?
        .unwrap_or_default();
    if !amplitude_modifiers.is_empty() && amplitude_modifiers.len() != octave_count as usize {
        return Err(LoadError::new(format!(
            "amplitude_modifiers had size {}, but octave_count was {octave_count}",
            amplitude_modifiers.len()
        )));
    }
    Ok(NoiseParameters {
        base_amplitude,
        base_octave,
        octave_count,
        normalize,
        amplitude_modifiers,
    })
}

/// `NormalNoise.Normalization.CODEC`: a bool or the string `"legacy"`.
fn parse_normalization(value: &Value) -> LoadResult<Normalization> {
    match value.as_str() {
        Some("legacy") => Ok(Normalization::Legacy),
        Some(other) => Err(LoadError::new(format!(
            "Invalid normalization type: {other}"
        ))),
        None => Ok(if parse_bool(value)? {
            Normalization::Enabled
        } else {
            Normalization::Disabled
        }),
    }
}
