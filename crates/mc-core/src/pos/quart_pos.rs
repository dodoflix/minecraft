//! `net.minecraft.core.QuartPos`: biome-resolution (4x4x4 block) coordinates.

pub const BITS: i32 = 2;
pub const SIZE: i32 = 4;
pub const MASK: i32 = 3;

pub const fn from_block(block_coord: i32) -> i32 {
    block_coord >> 2
}

pub const fn quart_local(block_coord: i32) -> i32 {
    block_coord & 3
}

pub const fn to_block(quart: i32) -> i32 {
    quart << 2
}

pub const fn from_section(section: i32) -> i32 {
    section << 2
}

pub const fn to_section(quart: i32) -> i32 {
    quart >> 2
}
