//! `net.minecraft.world.level.block.RailState`: connection logic of rails. Unlike vanilla the level is a parameter of
//! the methods that need it instead of a field (the struct is only a value: position, state and connections).

use super::impls::base_rail_block::{is_rail, is_rail_state, is_straight, shape_property};
use super::BlockStateExt;
use crate::server::level::ServerLevel;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Plane;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::RailShape;
use mc_data::props::EnumProperty;

pub struct RailState {
    pos: BlockPos,
    state: BlockState,
    is_straight: bool,
    shape_property: EnumProperty<RailShape>,
    connections: Vec<BlockPos>,
}

impl RailState {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        let shape_property = shape_property(state);
        let mut rail = Self { pos, state, is_straight: is_straight(state.block()), shape_property, connections: Vec::with_capacity(2) };
        rail.update_connections(state.get(shape_property));
        rail
    }

    pub fn get_connections(&self) -> &[BlockPos] {
        &self.connections
    }

    pub fn get_state(&self) -> BlockState {
        self.state
    }

    fn update_connections(&mut self, direction: RailShape) {
        let p = self.pos;
        self.connections.clear();
        let (a, b) = match direction {
            RailShape::NorthSouth => (p.north(), p.south()),
            RailShape::EastWest => (p.west(), p.east()),
            RailShape::AscendingEast => (p.west(), p.east().above()),
            RailShape::AscendingWest => (p.west().above(), p.east()),
            RailShape::AscendingNorth => (p.north().above(), p.south()),
            RailShape::AscendingSouth => (p.north(), p.south().above()),
            RailShape::SouthEast => (p.east(), p.south()),
            RailShape::SouthWest => (p.west(), p.south()),
            RailShape::NorthWest => (p.west(), p.north()),
            RailShape::NorthEast => (p.east(), p.north()),
        };
        self.connections.push(a);
        self.connections.push(b);
    }

    fn remove_soft_connections(&mut self, level: &dyn BlockGetter) {
        let mut i = 0;
        while i < self.connections.len() {
            match Self::get_rail(level, self.connections[i]) {
                Some(rail) if rail.connects_to(self) => {
                    self.connections[i] = rail.pos;
                    i += 1;
                }
                _ => {
                    self.connections.remove(i);
                }
            }
        }
    }

    fn has_rail(level: &dyn BlockGetter, pos: BlockPos) -> bool {
        is_rail(level, pos) || is_rail(level, pos.above()) || is_rail(level, pos.below())
    }

    fn get_rail(level: &dyn BlockGetter, pos: BlockPos) -> Option<RailState> {
        for test_pos in [pos, pos.above(), pos.below()] {
            let test_state = level.get_block_state(test_pos);
            if is_rail_state(test_state) {
                return Some(RailState::new(test_pos, test_state));
            }
        }
        None
    }

    fn connects_to(&self, rail: &RailState) -> bool {
        self.has_connection(rail.pos)
    }

    fn has_connection(&self, rail_pos: BlockPos) -> bool {
        self.connections.iter().any(|pos| pos.x() == rail_pos.x() && pos.z() == rail_pos.z())
    }

    pub fn count_potential_connections(&self, level: &dyn BlockGetter) -> i32 {
        Plane::Horizontal.faces().iter().filter(|d| Self::has_rail(level, self.pos.relative(**d))).count() as i32
    }

    fn can_connect_to(&self, rail: &RailState) -> bool {
        self.connects_to(rail) || self.connections.len() != 2
    }

    /// The ascending upgrade shared by `connectTo` and `place`.
    fn ascend(level: &dyn BlockGetter, shape: Option<RailShape>, north: BlockPos, south: BlockPos, east: BlockPos, west: BlockPos) -> Option<RailShape> {
        let mut shape = shape;
        if shape == Some(RailShape::NorthSouth) {
            if is_rail(level, north.above()) {
                shape = Some(RailShape::AscendingNorth);
            }
            if is_rail(level, south.above()) {
                shape = Some(RailShape::AscendingSouth);
            }
        }
        if shape == Some(RailShape::EastWest) {
            if is_rail(level, east.above()) {
                shape = Some(RailShape::AscendingEast);
            }
            if is_rail(level, west.above()) {
                shape = Some(RailShape::AscendingWest);
            }
        }
        shape
    }

    fn connect_to(&mut self, level: &mut ServerLevel, rail: &RailState) {
        self.connections.push(rail.pos);
        let (north, south, west, east) = (self.pos.north(), self.pos.south(), self.pos.west(), self.pos.east());
        let n = self.has_connection(north);
        let s = self.has_connection(south);
        let w = self.has_connection(west);
        let e = self.has_connection(east);
        let mut shape = None;
        if n || s {
            shape = Some(RailShape::NorthSouth);
        }
        if w || e {
            shape = Some(RailShape::EastWest);
        }
        if !self.is_straight {
            if s && e && !n && !w {
                shape = Some(RailShape::SouthEast);
            }
            if s && w && !n && !e {
                shape = Some(RailShape::SouthWest);
            }
            if n && w && !s && !e {
                shape = Some(RailShape::NorthWest);
            }
            if n && e && !s && !w {
                shape = Some(RailShape::NorthEast);
            }
        }
        let shape = Self::ascend(&*level, shape, north, south, east, west).unwrap_or(RailShape::NorthSouth);
        self.state = self.state.set(self.shape_property, shape);
        level.set_block_and_update(self.pos, self.state);
    }

    fn has_neighbor_rail(&self, level: &dyn BlockGetter, rail_pos: BlockPos) -> bool {
        match Self::get_rail(level, rail_pos) {
            None => false,
            Some(mut neighbor) => {
                neighbor.remove_soft_connections(level);
                neighbor.can_connect_to(self)
            }
        }
    }

    pub fn place(&mut self, level: &mut ServerLevel, has_signal: bool, first: bool, default_shape: RailShape) -> &RailState {
        let (north, south, west, east) = (self.pos.north(), self.pos.south(), self.pos.west(), self.pos.east());
        let n = self.has_neighbor_rail(&*level, north);
        let s = self.has_neighbor_rail(&*level, south);
        let w = self.has_neighbor_rail(&*level, west);
        let e = self.has_neighbor_rail(&*level, east);
        let mut shape = None;
        let north_or_south = n || s;
        let west_or_east = w || e;
        if north_or_south && !west_or_east {
            shape = Some(RailShape::NorthSouth);
        }
        if west_or_east && !north_or_south {
            shape = Some(RailShape::EastWest);
        }
        let south_and_east = s && e;
        let south_and_west = s && w;
        let north_and_east = n && e;
        let north_and_west = n && w;
        if !self.is_straight {
            if south_and_east && !n && !w {
                shape = Some(RailShape::SouthEast);
            }
            if south_and_west && !n && !e {
                shape = Some(RailShape::SouthWest);
            }
            if north_and_west && !s && !e {
                shape = Some(RailShape::NorthWest);
            }
            if north_and_east && !s && !w {
                shape = Some(RailShape::NorthEast);
            }
        }
        if shape.is_none() {
            if north_or_south && west_or_east {
                shape = Some(default_shape);
            } else if north_or_south {
                shape = Some(RailShape::NorthSouth);
            } else if west_or_east {
                shape = Some(RailShape::EastWest);
            }
            if !self.is_straight {
                if has_signal {
                    if south_and_east {
                        shape = Some(RailShape::SouthEast);
                    }
                    if south_and_west {
                        shape = Some(RailShape::SouthWest);
                    }
                    if north_and_east {
                        shape = Some(RailShape::NorthEast);
                    }
                    if north_and_west {
                        shape = Some(RailShape::NorthWest);
                    }
                } else {
                    if north_and_west {
                        shape = Some(RailShape::NorthWest);
                    }
                    if north_and_east {
                        shape = Some(RailShape::NorthEast);
                    }
                    if south_and_west {
                        shape = Some(RailShape::SouthWest);
                    }
                    if south_and_east {
                        shape = Some(RailShape::SouthEast);
                    }
                }
            }
        }
        let shape = Self::ascend(&*level, shape, north, south, east, west).unwrap_or(default_shape);
        self.update_connections(shape);
        self.state = self.state.set(self.shape_property, shape);
        if first || level.get_block_state(self.pos) != self.state {
            level.set_block_and_update(self.pos, self.state);
            for i in 0..self.connections.len() {
                if let Some(mut neighbor) = Self::get_rail(&*level, self.connections[i]) {
                    neighbor.remove_soft_connections(&*level);
                    if neighbor.can_connect_to(self) {
                        neighbor.connect_to(level, self);
                    }
                }
            }
        }
        self
    }
}
