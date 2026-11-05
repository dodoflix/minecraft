use super::{Coordinates, LocalCoordinates, WorldCoordinates, suggested_coordinates};
use crate::commands::shared_suggestion_provider::suggest_coordinates;
use crate::commands::{CommandSourceStack, Commands};
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue, StringReader, Suggestions,
    SuggestionsBuilder,
};
use mc_core::phys::Vec3;

/// `Vec3Argument`.
#[derive(Clone, Copy, Debug)]
pub struct Vec3Argument {
    center_correct: bool,
}

impl Vec3Argument {
    pub fn new(center_correct: bool) -> Self {
        Self { center_correct }
    }

    /// `vec3()`.
    pub fn vec3() -> Self {
        Self::new(true)
    }

    pub fn get_vec3(context: &CommandContext<CommandSourceStack>, name: &str) -> Vec3 {
        context.get_argument::<Coordinates>(name).get_position(context.get_source())
    }

    pub fn get_coordinates(context: &CommandContext<CommandSourceStack>, name: &str) -> Coordinates {
        *context.get_argument::<Coordinates>(name)
    }

    fn parse_coordinates(&self, reader: &mut StringReader) -> Result<Coordinates, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '^' {
            LocalCoordinates::parse(reader).map(Coordinates::Local)
        } else {
            WorldCoordinates::parse_double(reader, self.center_correct).map(Coordinates::World)
        }
    }
}

impl ArgumentType<CommandSourceStack> for Vec3Argument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse_coordinates(reader).map(value)
    }

    fn list_suggestions(&self, context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        let remainder = builder.get_remaining().to_owned();
        let coordinates = suggested_coordinates(&remainder, || context.get_source().get_absolute_coordinates());
        let this = *self;
        suggest_coordinates(&remainder, &coordinates, builder, Commands::create_validator(move |r| this.parse_coordinates(r).map(|_| ())))
    }

    fn get_examples(&self) -> Vec<String> {
        ["0 0 0", "~ ~ ~", "^ ^ ^", "^1 ^ ^-5", "0.1 -0.5 .9", "~0.5 ~1 ~-5"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "Vec3Argument".to_owned()
    }
}

crate::register_argument_type!(Vec3Argument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Vec3);
