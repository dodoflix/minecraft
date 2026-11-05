//! `net.minecraft.world.level.material.MapColor`: the 64 map color ids and `MapColor.Brightness`. Only what the server
//! observes is modelled (ids and packed ids); the RGB values are client rendering.

/// `MapColor.id` of the colors the server code names (the id of any block's color comes from `BlockState::get_map_color`).
pub mod map_color {
    pub const NONE: u8 = 0;
    pub const WATER: u8 = 12;
    pub const COLOR_ORANGE: u8 = 15;
    pub const COLOR_BROWN: u8 = 26;
}

/// `MapColor.Brightness`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Brightness {
    Low = 0,
    Normal = 1,
    High = 2,
    Lowest = 3,
}

/// `MapColor.getPackedId(brightness)`: `(byte)(id << 2 | brightness.id & 3)`.
pub fn get_packed_id(color: u8, brightness: Brightness) -> u8 {
    color << 2 | (brightness as u8 & 3)
}
