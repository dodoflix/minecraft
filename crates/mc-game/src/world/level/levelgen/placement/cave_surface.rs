//! `CaveSurface`: which side of a cave a feature grows from.

use crate::world::level::levelgen::codec::{LoadError, LoadResult, parse_str};
use mc_core::direction::Direction;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaveSurface {
    Ceiling,
    Floor,
}

impl CaveSurface {
    pub const ALL: [CaveSurface; 2] = [CaveSurface::Ceiling, CaveSurface::Floor];

    pub const fn get_direction(self) -> Direction {
        match self {
            CaveSurface::Ceiling => Direction::Up,
            CaveSurface::Floor => Direction::Down,
        }
    }

    pub const fn get_y(self) -> i32 {
        match self {
            CaveSurface::Ceiling => 1,
            CaveSurface::Floor => -1,
        }
    }

    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            CaveSurface::Ceiling => "ceiling",
            CaveSurface::Floor => "floor",
        }
    }

    /// `CaveSurface.CODEC` (`StringRepresentable.fromEnum`).
    pub fn from_json(value: &Value) -> LoadResult<CaveSurface> {
        let name = parse_str(value)?;
        Self::ALL
            .into_iter()
            .find(|s| s.get_serialized_name() == name)
            .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
    }
}
