//! `net.minecraft.commands.arguments.coordinates`: `~`/`^` coordinate arguments.

pub mod block_pos_argument;
pub mod column_pos_argument;
pub mod rotation_argument;
pub mod swizzle_argument;
pub mod vec2_argument;
pub mod vec3_argument;

pub use block_pos_argument::BlockPosArgument;
pub use column_pos_argument::{ColumnPos, ColumnPosArgument};
pub use rotation_argument::RotationArgument;
pub use swizzle_argument::SwizzleArgument;
pub use vec2_argument::Vec2Argument;
pub use vec3_argument::Vec3Argument;

use crate::commands::CommandSourceStack;
use crate::commands::message::simple_translatable;
use mc_brigadier::{CommandSyntaxException, ImmutableStringReader, SimpleCommandExceptionType, StringReader};
use mc_core::math::mth;
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::BlockPos;
use std::sync::LazyLock;

pub static ERROR_EXPECTED_DOUBLE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.pos.missing.double"));
pub static ERROR_NOT_COMPLETE_3D: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.pos3d.incomplete"));
pub static ERROR_MIXED_TYPE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.pos.mixed"));
pub static ERROR_NOT_COMPLETE_2D: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.pos2d.incomplete"));
pub static ERROR_ROTATION_INCOMPLETE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.rotation.incomplete"));
pub static ERROR_EXPECTED_INT: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.pos.missing.int"));

/// `WorldCoordinate`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorldCoordinate {
    pub relative: bool,
    pub value: f64,
}

impl WorldCoordinate {
    pub const fn new(relative: bool, value: f64) -> Self {
        Self { relative, value }
    }

    pub fn get(&self, original: f64) -> f64 {
        if self.relative { self.value + original } else { self.value }
    }

    pub fn parse_double(reader: &mut StringReader, center: bool) -> Result<WorldCoordinate, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '^' {
            Err(ERROR_MIXED_TYPE.create_with_context(reader))
        } else if !reader.can_read() {
            Err(ERROR_EXPECTED_DOUBLE.create_with_context(reader))
        } else {
            let relative = Self::is_relative(reader);
            let start = reader.get_cursor();
            let mut value = if reader.can_read() && reader.peek() != ' ' { reader.read_double()? } else { 0.0 };
            let number = mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor());
            if relative && number.is_empty() {
                Ok(WorldCoordinate::new(true, 0.0))
            } else {
                if !number.contains('.') && !relative && center {
                    value += 0.5;
                }
                Ok(WorldCoordinate::new(relative, value))
            }
        }
    }

    pub fn parse_int(reader: &mut StringReader) -> Result<WorldCoordinate, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '^' {
            Err(ERROR_MIXED_TYPE.create_with_context(reader))
        } else if !reader.can_read() {
            Err(ERROR_EXPECTED_INT.create_with_context(reader))
        } else {
            let relative = Self::is_relative(reader);
            let value = if reader.can_read() && reader.peek() != ' ' {
                if relative { reader.read_double()? } else { f64::from(reader.read_int()?) }
            } else {
                0.0
            };
            Ok(WorldCoordinate::new(relative, value))
        }
    }

    pub fn is_relative(reader: &mut StringReader) -> bool {
        if reader.peek() == '~' {
            reader.skip();
            true
        } else {
            false
        }
    }
}

/// `WorldCoordinates`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorldCoordinates {
    pub x: WorldCoordinate,
    pub y: WorldCoordinate,
    pub z: WorldCoordinate,
}

impl WorldCoordinates {
    pub fn zero_rotation() -> WorldCoordinates {
        Self::absolute_rotation(Vec2::new(0.0, 0.0))
    }

    pub fn parse_int(reader: &mut StringReader) -> Result<WorldCoordinates, CommandSyntaxException> {
        let start = reader.get_cursor();
        let x = WorldCoordinate::parse_int(reader)?;
        if reader.can_read() && reader.peek() == ' ' {
            reader.skip();
            let y = WorldCoordinate::parse_int(reader)?;
            if reader.can_read() && reader.peek() == ' ' {
                reader.skip();
                let z = WorldCoordinate::parse_int(reader)?;
                return Ok(WorldCoordinates { x, y, z });
            }
        }
        reader.set_cursor(start);
        Err(ERROR_NOT_COMPLETE_3D.create_with_context(reader))
    }

    pub fn parse_double(reader: &mut StringReader, center_correct: bool) -> Result<WorldCoordinates, CommandSyntaxException> {
        let start = reader.get_cursor();
        let x = WorldCoordinate::parse_double(reader, center_correct)?;
        if reader.can_read() && reader.peek() == ' ' {
            reader.skip();
            let y = WorldCoordinate::parse_double(reader, false)?;
            if reader.can_read() && reader.peek() == ' ' {
                reader.skip();
                let z = WorldCoordinate::parse_double(reader, center_correct)?;
                return Ok(WorldCoordinates { x, y, z });
            }
        }
        reader.set_cursor(start);
        Err(ERROR_NOT_COMPLETE_3D.create_with_context(reader))
    }

    pub fn absolute(x: f64, y: f64, z: f64) -> WorldCoordinates {
        WorldCoordinates { x: WorldCoordinate::new(false, x), y: WorldCoordinate::new(false, y), z: WorldCoordinate::new(false, z) }
    }

    pub fn absolute_rotation(rotation: Vec2) -> WorldCoordinates {
        WorldCoordinates {
            x: WorldCoordinate::new(false, f64::from(rotation.x)),
            y: WorldCoordinate::new(false, f64::from(rotation.y)),
            z: WorldCoordinate::new(true, 0.0),
        }
    }
}

/// `LocalCoordinates`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LocalCoordinates {
    pub left: f64,
    pub up: f64,
    pub forwards: f64,
}

impl LocalCoordinates {
    pub const PREFIX_LOCAL_COORDINATE: char = '^';

    pub fn apply(&self, source: Vec3, rotation: Vec2) -> Vec3 {
        let pi = std::f32::consts::PI / 180.0;
        let y_cos = mth::cos(f64::from((rotation.y + 90.0) * pi));
        let y_sin = mth::sin(f64::from((rotation.y + 90.0) * pi));
        let x_cos = mth::cos(f64::from(-rotation.x * pi));
        let x_sin = mth::sin(f64::from(-rotation.x * pi));
        let x_cos_up = mth::cos(f64::from((-rotation.x + 90.0) * pi));
        let x_sin_up = mth::sin(f64::from((-rotation.x + 90.0) * pi));
        let forwards = Vec3::new(f64::from(y_cos * x_cos), f64::from(x_sin), f64::from(y_sin * x_cos));
        let up = Vec3::new(f64::from(y_cos * x_cos_up), f64::from(x_sin_up), f64::from(y_sin * x_cos_up));
        let left = forwards.cross(up).scale(-1.0);
        source.add_xyz(
            forwards.x * self.forwards + up.x * self.up + left.x * self.left,
            forwards.y * self.forwards + up.y * self.up + left.y * self.left,
            forwards.z * self.forwards + up.z * self.up + left.z * self.left,
        )
    }

    pub fn parse(reader: &mut StringReader) -> Result<LocalCoordinates, CommandSyntaxException> {
        let start = reader.get_cursor();
        let left = Self::read_double(reader, start)?;
        if reader.can_read() && reader.peek() == ' ' {
            reader.skip();
            let up = Self::read_double(reader, start)?;
            if reader.can_read() && reader.peek() == ' ' {
                reader.skip();
                let forwards = Self::read_double(reader, start)?;
                return Ok(LocalCoordinates { left, up, forwards });
            }
        }
        reader.set_cursor(start);
        Err(ERROR_NOT_COMPLETE_3D.create_with_context(reader))
    }

    fn read_double(reader: &mut StringReader, start: usize) -> Result<f64, CommandSyntaxException> {
        if !reader.can_read() {
            Err(ERROR_EXPECTED_DOUBLE.create_with_context(reader))
        } else if reader.peek() != '^' {
            reader.set_cursor(start);
            Err(ERROR_MIXED_TYPE.create_with_context(reader))
        } else {
            reader.skip();
            if reader.can_read() && reader.peek() != ' ' { reader.read_double() } else { Ok(0.0) }
        }
    }
}

/// `Coordinates`: either world (`~`) or local (`^`) coordinates.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Coordinates {
    World(WorldCoordinates),
    Local(LocalCoordinates),
}

impl Coordinates {
    pub fn get_position(&self, sender: &CommandSourceStack) -> Vec3 {
        match self {
            Coordinates::World(c) => {
                let pos = sender.get_position();
                Vec3::new(c.x.get(pos.x), c.y.get(pos.y), c.z.get(pos.z))
            }
            Coordinates::Local(c) => c.apply(sender.get_anchor().apply_source(sender), sender.get_rotation()),
        }
    }

    pub fn get_rotation(&self, sender: &CommandSourceStack) -> Vec2 {
        match self {
            Coordinates::World(c) => {
                let rot = sender.get_rotation();
                Vec2::new(c.x.get(f64::from(rot.x)) as f32, c.y.get(f64::from(rot.y)) as f32)
            }
            Coordinates::Local(_) => Vec2::ZERO,
        }
    }

    pub fn get_block_pos(&self, sender: &CommandSourceStack) -> BlockPos {
        BlockPos::containing_position(&self.get_position(sender))
    }

    pub fn is_x_relative(&self) -> bool {
        match self {
            Coordinates::World(c) => c.x.relative,
            Coordinates::Local(_) => true,
        }
    }

    pub fn is_y_relative(&self) -> bool {
        match self {
            Coordinates::World(c) => c.y.relative,
            Coordinates::Local(_) => true,
        }
    }

    pub fn is_z_relative(&self) -> bool {
        match self {
            Coordinates::World(c) => c.z.relative,
            Coordinates::Local(_) => true,
        }
    }
}

impl From<WorldCoordinates> for Coordinates {
    fn from(c: WorldCoordinates) -> Self {
        Coordinates::World(c)
    }
}

impl From<LocalCoordinates> for Coordinates {
    fn from(c: LocalCoordinates) -> Self {
        Coordinates::Local(c)
    }
}

/// The `listSuggestions` body shared by the coordinate arguments.
pub(crate) fn suggested_coordinates(
    remainder: &str,
    relevant: impl FnOnce() -> Vec<crate::commands::shared_suggestion_provider::TextCoordinates>,
) -> Vec<crate::commands::shared_suggestion_provider::TextCoordinates> {
    if remainder.starts_with('^') {
        vec![crate::commands::shared_suggestion_provider::TextCoordinates::default_local()]
    } else {
        relevant()
    }
}
