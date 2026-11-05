//! `net.minecraft.server.commands.data`: the accessors `/data` (and `execute store`/`if data`) read and write NBT through.
pub mod block_data_accessor;
pub mod data_accessor;
pub mod entity_data_accessor;
pub mod storage_data_accessor;

pub use data_accessor::DataAccessor;
