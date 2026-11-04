use super::{Coordinates, ERROR_ROTATION_INCOMPLETE, WorldCoordinate, WorldCoordinates};
use crate::commands::CommandSourceStack;
use mc_brigadier::arguments::value;
use mc_brigadier::{ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue, StringReader};

/// `RotationArgument`.
#[derive(Clone, Copy, Debug, Default)]
pub struct RotationArgument;

impl RotationArgument {
    pub fn rotation() -> Self {
        Self
    }

    pub fn get_rotation(context: &CommandContext<CommandSourceStack>, name: &str) -> Coordinates {
        *context.get_argument::<Coordinates>(name)
    }
}

impl ArgumentType<CommandSourceStack> for RotationArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let start = reader.get_cursor();
        if !reader.can_read() {
            return Err(ERROR_ROTATION_INCOMPLETE.create_with_context(reader));
        }
        let y = WorldCoordinate::parse_double(reader, false)?;
        if reader.can_read() && reader.peek() == ' ' {
            reader.skip();
            let x = WorldCoordinate::parse_double(reader, false)?;
            Ok(value(Coordinates::World(WorldCoordinates { x, y, z: WorldCoordinate::new(true, 0.0) })))
        } else {
            reader.set_cursor(start);
            Err(ERROR_ROTATION_INCOMPLETE.create_with_context(reader))
        }
    }

    fn get_examples(&self) -> Vec<String> {
        ["0 0", "~ ~", "~-5 ~5"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "RotationArgument".to_owned()
    }
}
