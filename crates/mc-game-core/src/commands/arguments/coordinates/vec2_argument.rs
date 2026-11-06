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
use mc_core::phys::Vec2;

/// `Vec2Argument`.
#[derive(Clone, Copy, Debug)]
pub struct Vec2Argument {
    center_correct: bool,
}

impl Vec2Argument {
    pub fn new(center_correct: bool) -> Self {
        Self { center_correct }
    }

    /// `vec2()`.
    pub fn vec2() -> Self {
        Self::new(true)
    }

    pub fn get_vec2(context: &CommandContext<CommandSourceStack>, name: &str) -> Vec2 {
        let vec3 = context
            .get_argument::<Coordinates>(name)
            .get_position(context.get_source());
        Vec2::new(vec3.x as f32, vec3.z as f32)
    }

    fn parse_coordinates(
        &self,
        reader: &mut StringReader,
    ) -> Result<Coordinates, CommandSyntaxException> {
        let start = reader.get_cursor();
        if !reader.can_read() {
            return Err(ERROR_NOT_COMPLETE_2D.create_with_context(reader));
        }
        let x = WorldCoordinate::parse_double(reader, self.center_correct)?;
        if reader.can_read() && reader.peek() == ' ' {
            reader.skip();
            let z = WorldCoordinate::parse_double(reader, self.center_correct)?;
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

impl ArgumentType<CommandSourceStack> for Vec2Argument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse_coordinates(reader).map(value)
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        let remainder = builder.get_remaining().to_owned();
        let coordinates = suggested_coordinates(&remainder, || {
            context.get_source().get_absolute_coordinates()
        });
        let this = *self;
        suggest_2d_coordinates(
            &remainder,
            &coordinates,
            builder,
            Commands::create_validator(move |r| this.parse_coordinates(r).map(|_| ())),
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["0 0", "~ ~", "0.1 -0.5", "~1 ~-2"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "Vec2Argument".to_owned()
    }
}

crate::register_argument_type!(Vec2Argument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::Vec2
});
