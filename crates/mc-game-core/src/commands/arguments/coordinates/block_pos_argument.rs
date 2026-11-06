use super::{Coordinates, LocalCoordinates, WorldCoordinates, suggested_coordinates};
use crate::commands::host;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest_coordinates;
use crate::commands::{CommandSourceStack, Commands};
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue,
    SimpleCommandExceptionType, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::pos::BlockPos;
use std::sync::LazyLock;

pub static ERROR_NOT_LOADED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.pos.unloaded"));
pub static ERROR_OUT_OF_WORLD: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.pos.outofworld"));
pub static ERROR_OUT_OF_BOUNDS: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.pos.outofbounds"));

/// `BlockPosArgument`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockPosArgument;

impl BlockPosArgument {
    pub fn block_pos() -> Self {
        Self
    }

    pub fn get_block_pos(context: &CommandContext<CommandSourceStack>, name: &str) -> BlockPos {
        context
            .get_argument::<Coordinates>(name)
            .get_block_pos(context.get_source())
    }

    /// `getLoadedBlockPos(context, name)` for the source's level.
    pub fn get_loaded_block_pos(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<BlockPos, CommandSyntaxException> {
        Self::get_loaded_block_pos_in(context, context.get_source().get_level(), name)
    }

    pub fn get_loaded_block_pos_in(
        context: &CommandContext<CommandSourceStack>,
        level: &crate::world::level::DimensionKey,
        name: &str,
    ) -> Result<BlockPos, CommandSyntaxException> {
        let pos = Self::get_block_pos(context, name);
        context.get_source().with_server(|server| {
            if !host::level_has_chunk_at(server, level, pos) {
                Err(ERROR_NOT_LOADED.create())
            } else if !host::level_is_in_world_bounds(server, level, pos) {
                Err(ERROR_OUT_OF_WORLD.create())
            } else {
                Ok(pos)
            }
        })
    }

    pub fn get_spawnable_pos(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<BlockPos, CommandSyntaxException> {
        let pos = Self::get_block_pos(context, name);
        if !is_in_spawnable_bounds(pos) {
            Err(ERROR_OUT_OF_BOUNDS.create())
        } else {
            Ok(pos)
        }
    }
}

/// `Level.isInWorldBoundsHorizontal`.
pub fn is_in_world_bounds_horizontal(pos: BlockPos) -> bool {
    pos.x() >= -30_000_000 && pos.z() >= -30_000_000 && pos.x() < 30_000_000 && pos.z() < 30_000_000
}

/// `Level.isInSpawnableBounds`.
pub fn is_in_spawnable_bounds(pos: BlockPos) -> bool {
    !(pos.y() < -20_000_000 || pos.y() >= 20_000_000) && is_in_world_bounds_horizontal(pos)
}

impl ArgumentType<CommandSourceStack> for BlockPosArgument {
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
        suggest_coordinates(
            &remainder,
            &coordinates,
            builder,
            Commands::create_validator(|r| Self::parse_coordinates(r).map(|_| ())),
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["0 0 0", "~ ~ ~", "^ ^ ^", "^1 ^ ^-5", "~0.5 ~1 ~-5"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "BlockPosArgument".to_owned()
    }
}

impl BlockPosArgument {
    fn parse_coordinates(reader: &mut StringReader) -> Result<Coordinates, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '^' {
            LocalCoordinates::parse(reader).map(Coordinates::Local)
        } else {
            WorldCoordinates::parse_int(reader).map(Coordinates::World)
        }
    }
}

crate::register_argument_type!(BlockPosArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::BlockPos
});
