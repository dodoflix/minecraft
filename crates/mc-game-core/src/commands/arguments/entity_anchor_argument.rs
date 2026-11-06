//! `EntityAnchorArgument`.

use crate::commands::CommandSourceStack;
use crate::commands::message::dynamic_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType,
    ImmutableStringReader, ParsedValue, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::phys::Vec3;
use std::sync::LazyLock;

static ERROR_INVALID: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.anchor.invalid"));

/// `EntityAnchorArgument.Anchor`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Anchor {
    Feet,
    Eyes,
}

impl Anchor {
    pub const VALUES: [Anchor; 2] = [Anchor::Feet, Anchor::Eyes];

    pub fn name(self) -> &'static str {
        match self {
            Anchor::Feet => "feet",
            Anchor::Eyes => "eyes",
        }
    }

    pub fn get_by_name(name: &str) -> Option<Anchor> {
        Self::VALUES.into_iter().find(|a| a.name() == name)
    }

    /// `apply(Entity)` for an entity at `position` with the given eye height.
    pub fn apply_entity(self, position: Vec3, eye_height: f32) -> Vec3 {
        match self {
            Anchor::Feet => position,
            Anchor::Eyes => Vec3::new(position.x, position.y + f64::from(eye_height), position.z),
        }
    }

    /// `apply(CommandSourceStack)`.
    pub fn apply_source(self, source: &CommandSourceStack) -> Vec3 {
        match source.get_entity() {
            None => source.get_position(),
            Some(entity) => self.apply_entity(source.get_position(), entity.eye_height),
        }
    }
}

/// `EntityAnchorArgument`.
#[derive(Clone, Copy, Debug, Default)]
pub struct EntityAnchorArgument;

impl EntityAnchorArgument {
    pub fn anchor() -> Self {
        Self
    }

    pub fn get_anchor(context: &CommandContext<CommandSourceStack>, name: &str) -> Anchor {
        *context.get_argument::<Anchor>(name)
    }
}

impl ArgumentType<CommandSourceStack> for EntityAnchorArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let start = reader.get_cursor();
        let name = reader.read_unquoted_string();
        match Anchor::get_by_name(&name) {
            None => {
                reader.set_cursor(start);
                Err(ERROR_INVALID.create_with_context(reader, name))
            }
            Some(anchor) => Ok(value(anchor)),
        }
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        suggest(Anchor::VALUES.map(Anchor::name), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        vec!["eyes".to_owned(), "feet".to_owned()]
    }

    fn describe(&self) -> String {
        "EntityAnchorArgument".to_owned()
    }
}

crate::register_argument_type!(EntityAnchorArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::EntityAnchor
});
