//! `ParticleArgument`: `<particle type>[{options SNBT}]`.
use super::nbt_arguments::parse_tag_as_argument;
use super::simple_arguments::read_identifier;
use crate::commands::message::dynamic_translatable;
use crate::commands::shared_suggestion_provider::suggest_resource;
use crate::commands::CommandSourceStack;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader, ParsedValue, StringReader, Suggestions,
    SuggestionsBuilder,
};
use mc_data::ParticleType;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::ArgumentTypeTemplate;
use std::sync::LazyLock;

pub static ERROR_UNKNOWN_PARTICLE: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("particle.notFound"));
pub static ERROR_INVALID_OPTIONS: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("particle.invalidOptions"));

/// `ParticleArgument`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParticleArgument;

impl ParticleArgument {
    /// `particle(context)`
    pub fn particle() -> Self {
        Self
    }

    /// `getParticle`
    pub fn get_particle(context: &CommandContext<CommandSourceStack>, name: &str) -> ParticleOptions {
        context.get_argument::<ParticleOptions>(name).clone()
    }

    /// `readParticle(reader, registries)`
    pub fn read_particle(reader: &mut StringReader) -> Result<ParticleOptions, CommandSyntaxException> {
        let id = read_identifier(reader)?;
        let Some(particle_type) = ParticleType::by_name(&id.to_string()) else {
            return Err(ERROR_UNKNOWN_PARTICLE.create_with_context(reader, id.to_string()));
        };
        let extra_data = if reader.can_read() && reader.peek() == '{' { parse_tag_as_argument(reader)? } else { Tag::Compound(CompoundTag::new()) };
        ParticleOptions::parse_nbt(particle_type, &extra_data).map_err(|message| ERROR_INVALID_OPTIONS.create(message))
    }
}

impl ArgumentType<CommandSourceStack> for ParticleArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Self::read_particle(reader).map(value)
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest_resource(ParticleType::iter().map(|t| t.identifier()), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "particle{foo:bar}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ParticleArgument".to_owned()
    }
}

crate::register_argument_type!(ParticleArgument, |_a| ArgumentTypeTemplate::Particle);
