//! `MapDecoration`, `MapDecorationType` (the registry id is `mc_data::MapDecorationType`; this module adds its
//! `trackCount` flag) and `MapDecorationTypes` lookups.

use mc_protocol::components::DyeColor;
use mc_text::Component;

pub use mc_data::MapDecorationType;

/// `MapDecorationType.trackCount()`: set for the player markers, frames, red/blue markers and the banners
/// (`MapDecorationTypes.register(.., showOnItemFrame, trackCount)`).
pub fn track_count(ty: MapDecorationType) -> bool {
    ty == MapDecorationType::PLAYER
        || ty == MapDecorationType::FRAME
        || ty == by_name("red_marker")
        || ty == by_name("blue_marker")
        || ty == by_name("player_off_map")
        || ty == by_name("player_off_limits")
        || ty.name().starts_with("minecraft:banner_")
}

/// `MapDecorationTypes.<NAME>` by registry path.
pub fn by_name(path: &str) -> MapDecorationType {
    MapDecorationType::by_name(path).expect("vanilla map decoration type")
}

/// `MapBanner.getDecoration()`'s switch: the banner decoration of a base color.
pub fn banner_decoration(color: DyeColor) -> MapDecorationType {
    by_name(&format!("banner_{}", color.name()))
}

/// `MapDecoration` (`rot` is masked to 4 bits by the constructor).
#[derive(Clone, Debug, PartialEq)]
pub struct MapDecoration {
    pub ty: MapDecorationType,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    pub name: Option<Component>,
}

impl MapDecoration {
    pub fn new(ty: MapDecorationType, x: i8, y: i8, rot: i8, name: Option<Component>) -> Self {
        MapDecoration {
            ty,
            x,
            y,
            rot: rot & 15,
            name,
        }
    }

    pub fn to_packet(&self) -> mc_protocol::packets::play::MapDecoration {
        mc_protocol::packets::play::MapDecoration {
            decoration_type: self.ty,
            x: self.x,
            y: self.y,
            rot: self.rot,
            name: self.name.clone(),
        }
    }
}
