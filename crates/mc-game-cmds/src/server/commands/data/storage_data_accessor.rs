//! `StorageDataAccessor`.
use super::DataAccessor;
use super::data_accessor::{format_scale, to_pretty_component};
use crate::server::commands::arg_provider::ArgProvider;
use mc_brigadier::CommandSyntaxException;
use mc_core::ident::Identifier;
use mc_game_core::commands::arguments::{IdentifierArgument, NbtPath};
use mc_game_core::commands::shared_suggestion_provider::suggest_resource;
use mc_game_core::commands::{CommandSourceStack, argument, host};
use mc_game_core::translatable;
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;

pub struct StorageDataAccessor {
    id: Identifier,
}

/// `StorageDataAccessor.PROVIDER`.
pub fn provider(arg: &'static str) -> ArgProvider<Box<dyn DataAccessor>> {
    ArgProvider::create(
        "storage",
        move || {
            argument(arg, IdentifierArgument::id()).suggests(|c, builder| {
                Ok(suggest_resource(
                    c.get_source().with_server(host::command_storage_keys),
                    builder,
                ))
            })
        },
        move |c| {
            Ok(Box::new(StorageDataAccessor {
                id: IdentifierArgument::get_id(c, arg),
            }) as Box<dyn DataAccessor>)
        },
    )
}

impl StorageDataAccessor {
    /// `Component.translationArg(id)`.
    fn id_arg(&self) -> Component {
        Component::literal(self.id.to_string())
    }
}

impl DataAccessor for StorageDataAccessor {
    fn set_data(
        &self,
        source: &CommandSourceStack,
        tag: CompoundTag,
    ) -> Result<(), CommandSyntaxException> {
        source.with_server(|server| host::command_storage_set(server, &self.id, tag));
        Ok(())
    }

    fn get_data(&self, source: &CommandSourceStack) -> Result<CompoundTag, CommandSyntaxException> {
        Ok(source.with_server(|server| host::command_storage_get(server, &self.id)))
    }

    fn get_modified_success(&self, _source: &CommandSourceStack) -> Component {
        translatable!("commands.data.storage.modified", self.id_arg())
    }

    fn get_print_success(&self, _source: &CommandSourceStack, data: &Tag) -> Component {
        translatable!(
            "commands.data.storage.query",
            self.id_arg(),
            to_pretty_component(data)
        )
    }

    fn get_print_success_for_path(
        &self,
        _source: &CommandSourceStack,
        path: &NbtPath,
        scale: f64,
        value: i32,
    ) -> Component {
        translatable!(
            "commands.data.storage.get",
            path.as_string(),
            self.id_arg(),
            format_scale(scale),
            value
        )
    }
}
