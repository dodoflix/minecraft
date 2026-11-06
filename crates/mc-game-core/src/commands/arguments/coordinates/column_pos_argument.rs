use super::{
    Coordinates, ERROR_NOT_COMPLETE_2D, WorldCoordinate, WorldCoordinates, suggested_coordinates,
};
use crate::commands::shared_suggestion_provider::suggest_2d_coordinates;
use crate::commands::{CommandSourceStack, Commands};
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue,
    StringReader, Suggestions, SuggestionsBuilder,
};

/// `ColumnPos`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ColumnPos {
    pub x: i32,
    pub z: i32,
}

/// `ColumnPosArgument`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ColumnPosArgument;

impl ColumnPosArgument {
    pub fn column_pos() -> Self {
        Self
    }

    pub fn get_column_pos(context: &CommandContext<CommandSourceStack>, name: &str) -> ColumnPos {
        let pos = context
            .get_argument::<Coordinates>(name)
            .get_block_pos(context.get_source());
        ColumnPos {
            x: pos.x(),
            z: pos.z(),
        }
    }

    fn parse_coordinates(reader: &mut StringReader) -> Result<Coordinates, CommandSyntaxException> {
        let start = reader.get_cursor();
        if !reader.can_read() {
            return Err(ERROR_NOT_COMPLETE_2D.create_with_context(reader));
        }
        let x = WorldCoordinate::parse_int(reader)?;
        if reader.can_read() && reader.peek() == ' ' {
            reader.skip();
            let z = WorldCoordinate::parse_int(reader)?;
            Ok(Coordinates::World(WorldCoordinates {
                x,
                y: WorldCoordinate::new(true, 0.0),
                z,
            }))
        } else {
            reader.set_cursor(start);
            Err(ERROR_NOT_COMPLETE_2D.create_with_context(reader))
        }
    }
}

impl ArgumentType<CommandSourceStack> for ColumnPosArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Self::parse_coordinates(reader).map(value)
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        let remainder = builder.get_remaining().to_owned();
        let coordinates = suggested_coordinates(&remainder, || {
            context.get_source().get_relevant_coordinates()
        });
        suggest_2d_coordinates(
            &remainder,
            &coordinates,
            builder,
            Commands::create_validator(|r| Self::parse_coordinates(r).map(|_| ())),
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["0 0", "~ ~", "~1 ~-2", "^ ^", "^-1 ^0"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "ColumnPosArgument".to_owned()
    }
}

crate::register_argument_type!(ColumnPosArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::ColumnPos
});
