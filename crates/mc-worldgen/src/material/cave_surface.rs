//! `net.minecraft.world.level.levelgen.placement.CaveSurface`.

use mc_core::direction::Direction;
use serde_json::Value;

use crate::data::LoadError;
use crate::densityfunction::codec::{LoadResult, parse_str};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaveSurface {
    Ceiling,
    Floor,
}

impl CaveSurface {
    /// `values()`.
    pub const ALL: [CaveSurface; 2] = [CaveSurface::Ceiling, CaveSurface::Floor];

    /// `getDirection()`.
    pub fn direction(self) -> Direction {
        match self {
            CaveSurface::Ceiling => Direction::Up,
            CaveSurface::Floor => Direction::Down,
        }
    }

    /// `getY()`.
    pub fn y(self) -> i32 {
        match self {
            CaveSurface::Ceiling => 1,
            CaveSurface::Floor => -1,
        }
    }

    /// `getSerializedName()`.
    pub fn serialized_name(self) -> &'static str {
        match self {
            CaveSurface::Ceiling => "ceiling",
            CaveSurface::Floor => "floor",
        }
    }

    /// `CaveSurface.CODEC` (`StringRepresentable.fromEnum`).
    pub fn from_json(value: &Value) -> LoadResult<CaveSurface> {
        let name = parse_str(value)?;
        CaveSurface::ALL
            .into_iter()
            .find(|surface| surface.serialized_name() == name)
            .ok_or_else(|| LoadError::new(format!("Unknown element name: {name}")))
    }
}
