//! `world.level.block.Rotation`. The codecs are not ported here.

use crate::direction::{Direction, shuffled_copy};
use crate::math::mth;
use crate::math::octahedral_group::OctahedralGroup;
use crate::random::RandomSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

impl Rotation {
    pub const ALL: [Rotation; 4] = [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Counterclockwise90,
    ];

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `getIndex`
    pub const fn get_index(self) -> i32 {
        self as i32
    }

    /// `BY_ID` (`ByIdMap.continuous(.., WRAP)`)
    pub fn by_id(id: i32) -> Rotation {
        Self::ALL[mth::positive_modulo_i32(id, 4) as usize]
    }

    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            Rotation::None => "none",
            Rotation::Clockwise90 => "clockwise_90",
            Rotation::Clockwise180 => "180",
            Rotation::Counterclockwise90 => "counterclockwise_90",
        }
    }

    /// `CODEC` lookup by serialized name.
    pub fn by_name(name: &str) -> Option<Rotation> {
        Self::ALL
            .into_iter()
            .find(|r| r.get_serialized_name() == name)
    }

    /// `rotation()`
    pub const fn rotation(self) -> OctahedralGroup {
        match self {
            Rotation::None => OctahedralGroup::Identity,
            Rotation::Clockwise90 => OctahedralGroup::BLOCK_ROT_Y_90,
            Rotation::Clockwise180 => OctahedralGroup::Rot180FaceXz,
            Rotation::Counterclockwise90 => OctahedralGroup::BLOCK_ROT_Y_270,
        }
    }

    /// `getRotated(Rotation)`: composition `self` then `rot`.
    pub const fn get_rotated(self, rot: Rotation) -> Rotation {
        Self::ALL[(self as usize + rot as usize) % 4]
    }

    /// `rotate(Direction)`
    pub fn rotate(self, direction: Direction) -> Direction {
        if direction.get_axis() == crate::direction::Axis::Y {
            return direction;
        }
        match self {
            Rotation::Clockwise90 => direction.get_clockwise(),
            Rotation::Clockwise180 => direction.get_opposite(),
            Rotation::Counterclockwise90 => direction.get_counter_clockwise(),
            Rotation::None => direction,
        }
    }

    /// `rotate(int rotation, int steps)`
    pub fn rotate_steps(self, rotation: i32, steps: i32) -> i32 {
        match self {
            Rotation::Clockwise90 => rotation.wrapping_add(steps / 4).wrapping_rem(steps),
            Rotation::Clockwise180 => rotation.wrapping_add(steps / 2).wrapping_rem(steps),
            Rotation::Counterclockwise90 => rotation
                .wrapping_add(steps.wrapping_mul(3) / 4)
                .wrapping_rem(steps),
            Rotation::None => rotation,
        }
    }

    pub fn get_random(random: &mut dyn RandomSource) -> Rotation {
        Self::ALL[random.next_int_bounded(4) as usize]
    }

    /// `getShuffled`
    pub fn get_shuffled(random: &mut dyn RandomSource) -> Vec<Rotation> {
        shuffled_copy(&Self::ALL, random)
    }
}
