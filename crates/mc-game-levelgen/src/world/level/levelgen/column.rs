//! `net.minecraft.world.level.levelgen.Column`: a vertical run of "inside" blocks bounded by a floor and/or ceiling.

use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_game_core::world::level::BlockGetter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    /// `Column.Line`: unbounded in both directions.
    Line,
    /// `Column.Ray`: bounded on one side by `edge` (the floor when `pointing_up`, else the ceiling).
    Ray { edge: i32, pointing_up: bool },
    /// `Column.Range`.
    Range(Range),
}

/// `Column.Range`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    floor: i32,
    ceiling: i32,
}

impl Range {
    /// `new Range(floor, ceiling)`; vanilla throws `IllegalArgumentException` for a negative height.
    pub fn new(floor: i32, ceiling: i32) -> Range {
        let range = Range { floor, ceiling };
        assert!(range.height() >= 0, "Column of negative height: {range:?}");
        range
    }

    pub fn ceiling(&self) -> i32 {
        self.ceiling
    }

    pub fn floor(&self) -> i32 {
        self.floor
    }

    pub fn height(&self) -> i32 {
        self.ceiling.wrapping_sub(self.floor).wrapping_sub(1)
    }
}

impl Column {
    pub fn around(lowest: i32, highest: i32) -> Range {
        Range::new(lowest - 1, highest + 1)
    }

    pub fn inside(floor: i32, ceiling: i32) -> Range {
        Range::new(floor, ceiling)
    }

    pub fn below(ceiling: i32) -> Column {
        Column::Ray {
            edge: ceiling,
            pointing_up: false,
        }
    }

    pub fn from_highest(highest: i32) -> Column {
        Column::Ray {
            edge: highest + 1,
            pointing_up: false,
        }
    }

    pub fn above(floor: i32) -> Column {
        Column::Ray {
            edge: floor,
            pointing_up: true,
        }
    }

    pub fn from_lowest(lowest: i32) -> Column {
        Column::Ray {
            edge: lowest - 1,
            pointing_up: true,
        }
    }

    pub fn line() -> Column {
        Column::Line
    }

    /// `create(floor, ceiling)`.
    pub fn create(floor: Option<i32>, ceiling: Option<i32>) -> Column {
        match (floor, ceiling) {
            (Some(floor), Some(ceiling)) => Column::Range(Column::inside(floor, ceiling)),
            (Some(floor), None) => Column::above(floor),
            (None, Some(ceiling)) => Column::below(ceiling),
            (None, None) => Column::line(),
        }
    }

    pub fn get_ceiling(&self) -> Option<i32> {
        match *self {
            Column::Line => None,
            Column::Ray { edge, pointing_up } => (!pointing_up).then_some(edge),
            Column::Range(range) => Some(range.ceiling),
        }
    }

    pub fn get_floor(&self) -> Option<i32> {
        match *self {
            Column::Line => None,
            Column::Ray { edge, pointing_up } => pointing_up.then_some(edge),
            Column::Range(range) => Some(range.floor),
        }
    }

    pub fn get_height(&self) -> Option<i32> {
        match *self {
            Column::Range(range) => Some(range.height()),
            _ => None,
        }
    }

    pub fn with_floor(&self, floor: Option<i32>) -> Column {
        Column::create(floor, self.get_ceiling())
    }

    pub fn with_ceiling(&self, ceiling: Option<i32>) -> Column {
        Column::create(self.get_floor(), ceiling)
    }

    /// `scan(level, pos, searchRange, insideColumn, validEdge)`.
    pub fn scan(
        level: &dyn BlockGetter,
        pos: BlockPos,
        search_range: i32,
        inside_column: impl Fn(BlockState) -> bool,
        valid_edge: impl Fn(BlockState) -> bool,
    ) -> Option<Column> {
        if !inside_column(level.get_block_state(pos)) {
            return None;
        }
        let nearest_empty_y = pos.y();
        let scan_direction = |step: i32| {
            let mut y = nearest_empty_y;
            let mut i = 1;
            while i < search_range
                && inside_column(level.get_block_state(BlockPos::new(pos.x(), y, pos.z())))
            {
                y += step;
                i += 1;
            }
            valid_edge(level.get_block_state(BlockPos::new(pos.x(), y, pos.z()))).then_some(y)
        };
        let ceiling = scan_direction(1);
        let floor = scan_direction(-1);
        Some(Column::create(floor, ceiling))
    }
}
