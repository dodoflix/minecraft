//! `BlockDataAccessor`.
use super::DataAccessor;
use super::data_accessor::{format_scale, to_pretty_component};
use crate::commands::arguments::coordinates::block_pos_argument::BlockPosArgument;
use crate::commands::arguments::NbtPath;
use crate::commands::message::simple_translatable;
use crate::commands::{CommandSourceStack, argument, host};
use crate::server::commands::arg_provider::ArgProvider;
use crate::translatable;
use crate::world::level::DimensionKey;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::pos::BlockPos;
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_NOT_A_BLOCK_ENTITY: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.data.block.invalid"));

pub struct BlockDataAccessor {
    level: DimensionKey,
    pos: BlockPos,
}

/// `BlockDataAccessor.PROVIDER`.
pub fn provider(arg: &'static str) -> ArgProvider<Box<dyn DataAccessor>> {
    ArgProvider::create(
        "block",
        move || argument(arg, BlockPosArgument::block_pos()),
        move |c| {
            let pos = BlockPosArgument::get_loaded_block_pos(c, arg)?;
            let level = c.get_source().get_level().clone();
            if !c.get_source().with_server(|server| host::block_entity_exists(server, &level, pos)) {
                return Err(ERROR_NOT_A_BLOCK_ENTITY.create());
            }
            Ok(Box::new(BlockDataAccessor { level, pos }) as Box<dyn DataAccessor>)
        },
    )
}

impl DataAccessor for BlockDataAccessor {
    fn set_data(&self, source: &CommandSourceStack, tag: CompoundTag) -> Result<(), CommandSyntaxException> {
        source.with_server(|server| host::block_entity_load_data(server, &self.level, self.pos, &tag));
        Ok(())
    }

    fn get_data(&self, source: &CommandSourceStack) -> Result<CompoundTag, CommandSyntaxException> {
        Ok(source.with_server(|server| host::block_entity_data(server, &self.level, self.pos)).unwrap_or_default())
    }

    fn get_modified_success(&self, _source: &CommandSourceStack) -> Component {
        translatable!("commands.data.block.modified", self.pos.x(), self.pos.y(), self.pos.z())
    }

    fn get_print_success(&self, _source: &CommandSourceStack, data: &Tag) -> Component {
        translatable!("commands.data.block.query", self.pos.x(), self.pos.y(), self.pos.z(), to_pretty_component(data))
    }

    fn get_print_success_for_path(&self, _source: &CommandSourceStack, path: &NbtPath, scale: f64, value: i32) -> Component {
        translatable!("commands.data.block.get", path.as_string(), self.pos.x(), self.pos.y(), self.pos.z(), format_scale(scale), value)
    }
}
