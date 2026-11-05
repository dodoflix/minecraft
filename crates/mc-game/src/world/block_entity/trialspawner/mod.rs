//! `net.minecraft.world.level.block.entity.trialspawner`.

pub mod config;
pub mod player_detector;
pub mod state;
pub mod state_data;
pub mod trial_spawner;

pub use config::{ConfigHolder, FullConfig, TrialSpawnerConfig};
pub use player_detector::PlayerDetector;
pub use state::TrialSpawnerStateExt;
pub use state_data::TrialSpawnerStateData;
pub use trial_spawner::TrialSpawner;
