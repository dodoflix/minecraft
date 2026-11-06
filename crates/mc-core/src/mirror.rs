//! `world.level.block.Mirror`. `symbol()` (`Component`) and the codecs are not ported here.

use crate::direction::{Axis, Direction};
use crate::math::octahedral_group::OctahedralGroup;
use crate::rotation::Rotation;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mirror {
    None,
    LeftRight,
    FrontBack,
}

impl Mirror {
    pub const ALL: [Mirror; 3] = [Mirror::None, Mirror::LeftRight, Mirror::FrontBack];

    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            Mirror::None => "none",
            Mirror::LeftRight => "left_right",
            Mirror::FrontBack => "front_back",
        }
    }

    /// `rotation()`
    pub const fn rotation(self) -> OctahedralGroup {
        match self {
            Mirror::None => OctahedralGroup::Identity,
            Mirror::LeftRight => OctahedralGroup::InvertZ,
            Mirror::FrontBack => OctahedralGroup::InvertX,
        }
    }

    /// `CODEC` lookup by serialized name.
    pub fn by_name(name: &str) -> Option<Mirror> {
        Self::ALL
            .into_iter()
            .find(|m| m.get_serialized_name() == name)
    }

    /// `mirror(int rotation, int steps)`
    pub fn mirror_steps(self, rotation: i32, steps: i32) -> i32 {
        let half_steps = steps / 2;
        let corrected_rotation = if rotation > half_steps {
            rotation.wrapping_sub(steps)
        } else {
            rotation
        };
        match self {
            Mirror::LeftRight => half_steps
                .wrapping_sub(corrected_rotation)
                .wrapping_add(steps)
                .wrapping_rem(steps),
            Mirror::FrontBack => steps.wrapping_sub(corrected_rotation).wrapping_rem(steps),
            Mirror::None => rotation,
        }
    }

    /// `getRotation(Direction)`
    pub fn get_rotation(self, value: Direction) -> Rotation {
        let axis = value.get_axis();
        if (self != Mirror::LeftRight || axis != Axis::Z)
            && (self != Mirror::FrontBack || axis != Axis::X)
        {
            Rotation::None
        } else {
            Rotation::Clockwise180
        }
    }

    /// `mirror(Direction)`
    pub fn mirror(self, direction: Direction) -> Direction {
        let axis = direction.get_axis();
        if (self == Mirror::FrontBack && axis == Axis::X)
            || (self == Mirror::LeftRight && axis == Axis::Z)
        {
            direction.get_opposite()
        } else {
            direction
        }
    }
}
