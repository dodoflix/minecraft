//! `net.minecraft.world.level.block.entity.vault`.

pub mod config;
pub mod server_data;
pub mod shared_data;
pub mod state;

pub use config::VaultConfig;
pub use server_data::VaultServerData;
pub use shared_data::VaultSharedData;
pub use state::VaultStateExt;
