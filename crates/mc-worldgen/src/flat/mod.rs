//! Flat world generator: layer settings, presets and `FlatLevelSource`.

pub mod layer_info;
pub mod level_source;
pub mod presets;
pub mod settings;

pub use layer_info::FlatLayerInfo;
pub use level_source::FlatLevelSource;
pub use presets::FlatLevelGeneratorPreset;
pub use settings::FlatLevelGeneratorSettings;
