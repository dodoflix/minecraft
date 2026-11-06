//! `Orientation` (redstone wire evaluation order of the `redstone_experiments` feature): an (up, front, side bias)
//! triple, 48 values. Vanilla precomputes `withFront`/`withUp`/`withSideBias` tables; here they are computed on the
//! fly from the same generation rules (`generateContext`), which yields identical results.

use mc_core::direction::{Axis, Direction};
use mc_core::random::RandomSource;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SideBias {
    Left,
    Right,
}

impl SideBias {
    pub fn get_opposite(self) -> SideBias {
        match self {
            SideBias::Left => SideBias::Right,
            SideBias::Right => SideBias::Left,
        }
    }
    const fn ordinal(self) -> usize {
        self as usize
    }
    /// `toString` (the lowercase name).
    pub fn name(self) -> &'static str {
        match self {
            SideBias::Left => "left",
            SideBias::Right => "right",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Orientation {
    up: Direction,
    front: Direction,
    side: Direction,
    side_bias: SideBias,
    index: usize,
}

/// `Orientation.generateIndex`; panics when up and front share an axis (vanilla throws `IllegalStateException`).
fn generate_index(up: Direction, front: Direction, side_bias: SideBias) -> usize {
    assert!(
        up.get_axis() != front.get_axis(),
        "Up-vector and front-vector can not be on the same axis"
    );
    let front_axis_key = if up.get_axis() == Axis::Y {
        usize::from(front.get_axis() == Axis::X)
    } else {
        usize::from(front.get_axis() == Axis::Y)
    };
    let front_key = front_axis_key << 1 | front.get_axis_direction() as usize;
    ((up.ordinal() << 2) + front_key << 1) + side_bias.ordinal()
}

impl Orientation {
    fn new(up: Direction, front: Direction, side_bias: SideBias) -> Orientation {
        let index = generate_index(up, front, side_bias);
        // front x up
        let (f, u) = (front.get_unit_vec3i(), up.get_unit_vec3i());
        let right = (
            f.y() * u.z() - f.z() * u.y(),
            f.z() * u.x() - f.x() * u.z(),
            f.x() * u.y() - f.y() * u.x(),
        );
        let side = Direction::get_nearest(right.0, right.1, right.2, None)
            .expect("front and up are perpendicular");
        let side = if side_bias == SideBias::Right {
            side
        } else {
            side.get_opposite()
        };
        Orientation {
            up,
            front,
            side,
            side_bias,
            index,
        }
    }

    pub fn of(up: Direction, front: Direction, side_bias: SideBias) -> Orientation {
        Orientation::new(up, front, side_bias)
    }

    pub fn with_up(&self, up: Direction) -> Orientation {
        let mut front = self.front;
        if up == self.front {
            front = self.up.get_opposite();
        }
        if up == self.front.get_opposite() {
            front = self.up;
        }
        Orientation::new(up, front, self.side_bias)
    }

    pub fn with_front(&self, front: Direction) -> Orientation {
        let mut up = self.up;
        if front == self.up {
            up = self.front.get_opposite();
        }
        if front == self.up.get_opposite() {
            up = self.front;
        }
        Orientation::new(up, front, self.side_bias)
    }

    pub fn with_front_preserve_up(&self, front: Direction) -> Orientation {
        if front.get_axis() == self.up.get_axis() {
            *self
        } else {
            self.with_front(front)
        }
    }

    pub fn with_front_adjust_side_bias(&self, front: Direction) -> Orientation {
        let with_front = self.with_front(front);
        if self.front == with_front.side {
            with_front.with_mirror()
        } else {
            with_front
        }
    }

    pub fn with_side_bias(&self, side_bias: SideBias) -> Orientation {
        Orientation::new(self.up, self.front, side_bias)
    }

    pub fn with_mirror(&self) -> Orientation {
        self.with_side_bias(self.side_bias.get_opposite())
    }

    pub fn get_front(&self) -> Direction {
        self.front
    }
    pub fn get_up(&self) -> Direction {
        self.up
    }
    pub fn get_side(&self) -> Direction {
        self.side
    }
    pub fn get_side_bias(&self) -> SideBias {
        self.side_bias
    }

    /// `getDirections()`: back, front, side, other side, down(rel.), up(rel.).
    pub fn get_directions(&self) -> [Direction; 6] {
        [
            self.front.get_opposite(),
            self.front,
            self.side,
            self.side.get_opposite(),
            self.up.get_opposite(),
            self.up,
        ]
    }

    pub fn get_horizontal_directions(&self) -> Vec<Direction> {
        self.get_directions()
            .into_iter()
            .filter(|d| d.get_axis() != self.up.get_axis())
            .collect()
    }

    pub fn get_vertical_directions(&self) -> Vec<Direction> {
        self.get_directions()
            .into_iter()
            .filter(|d| d.get_axis() == self.up.get_axis())
            .collect()
    }

    pub fn get_index(&self) -> usize {
        self.index
    }

    pub fn from_index(index: usize) -> Orientation {
        all()[index]
    }

    /// `Orientation.random(rand)`: `Util.getRandom(ORIENTATIONS, rand)` = one `nextInt(48)`.
    pub fn random(rand: &mut dyn RandomSource) -> Orientation {
        all()[rand.next_int_bounded(48) as usize]
    }
}

impl std::fmt::Display for Orientation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[up={},front={},sideBias={}]",
            self.up.get_name(),
            self.front.get_name(),
            self.side_bias.name()
        )
    }
}

fn all() -> &'static [Orientation; 48] {
    static ALL: OnceLock<[Orientation; 48]> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut table: [Option<Orientation>; 48] = [None; 48];
        for up in Direction::ALL {
            for front in Direction::ALL {
                if up.get_axis() == front.get_axis() {
                    continue;
                }
                for bias in [SideBias::Left, SideBias::Right] {
                    let o = Orientation::new(up, front, bias);
                    table[o.index] = Some(o);
                }
            }
        }
        table.map(|o| o.expect("48 orientations"))
    })
}
