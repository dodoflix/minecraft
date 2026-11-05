//! `Block.UPDATE_*` flags of `Level.setBlock` (`@Block.UpdateFlags`).
pub const UPDATE_NEIGHBORS: u32 = 1;
pub const UPDATE_CLIENTS: u32 = 2;
pub const UPDATE_INVISIBLE: u32 = 4;
pub const UPDATE_IMMEDIATE: u32 = 8;
pub const UPDATE_KNOWN_SHAPE: u32 = 16;
pub const UPDATE_SUPPRESS_DROPS: u32 = 32;
pub const UPDATE_MOVE_BY_PISTON: u32 = 64;
pub const UPDATE_SKIP_SHAPE_UPDATE_ON_WIRE: u32 = 128;
pub const UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS: u32 = 256;
pub const UPDATE_SKIP_ON_PLACE: u32 = 512;
pub const UPDATE_NONE: u32 = 260;
pub const UPDATE_ALL: u32 = 3;
pub const UPDATE_ALL_IMMEDIATE: u32 = 11;
pub const UPDATE_SKIP_ALL_SIDEEFFECTS: u32 = 816;
/// `Block.UPDATE_LIMIT` (max recursion depth of neighbor/shape updates).
pub const UPDATE_LIMIT: i32 = 512;
