//! `EntityDataAccessor`.
use super::DataAccessor;
use super::data_accessor::{format_scale, to_pretty_component};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, NbtPath};
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, argument, host};
use crate::server::commands::arg_provider::ArgProvider;
use mc_game_core::translatable;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_NO_PLAYERS: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.data.entity.invalid"));

pub struct EntityDataAccessor {
    entity: EntityRef,
}

/// `EntityDataAccessor.PROVIDER`.
pub fn provider(arg: &'static str) -> ArgProvider<Box<dyn DataAccessor>> {
    ArgProvider::create(
        "entity",
        move || argument(arg, EntityArgument::entity()),
        move |c| Ok(Box::new(EntityDataAccessor::new(EntityArgument::get_entity(c, arg)?)) as Box<dyn DataAccessor>),
    )
}

impl EntityDataAccessor {
    pub fn new(entity: EntityRef) -> Self {
        Self { entity }
    }

    fn display_name(&self, source: &CommandSourceStack) -> Component {
        source.with_server(|server| {
            host::get_entity(server, &self.entity).map_or_else(Component::empty, |e| host::entity_display_name(server, e))
        })
    }
}

impl DataAccessor for EntityDataAccessor {
    fn set_data(&self, source: &CommandSourceStack, tag: CompoundTag) -> Result<(), CommandSyntaxException> {
        source.with_server(|server| {
            if host::get_entity(server, &self.entity).is_some_and(|e| e.is_player()) {
                return Err(ERROR_NO_PLAYERS.create());
            }
            // vanilla: `Entity.load` failing is a crash report (the command fails with a logged exception)
            if let Err(reason) = host::entity_load_data(server, &self.entity, &tag) {
                panic!("{reason}");
            }
            Ok(())
        })
    }

    fn get_data(&self, source: &CommandSourceStack) -> Result<CompoundTag, CommandSyntaxException> {
        Ok(source.with_server(|server| host::entity_data_with_selected_item(server, &self.entity)).unwrap_or_default())
    }

    fn get_modified_success(&self, source: &CommandSourceStack) -> Component {
        translatable!("commands.data.entity.modified", self.display_name(source))
    }

    fn get_print_success(&self, source: &CommandSourceStack, data: &Tag) -> Component {
        translatable!("commands.data.entity.query", self.display_name(source), to_pretty_component(data))
    }

    fn get_print_success_for_path(&self, source: &CommandSourceStack, path: &NbtPath, scale: f64, value: i32) -> Component {
        translatable!("commands.data.entity.get", path.as_string(), self.display_name(source), format_scale(scale), value)
    }
}
