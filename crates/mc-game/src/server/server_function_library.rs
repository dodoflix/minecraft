//! `net.minecraft.server.ServerFunctionLibrary`: every `data/<ns>/function/**.mcfunction` of the selected packs, compiled
//! against the command dispatcher of the reload, plus the `tags/function` tags over them.

use super::packs::{FileToIdConverter, ResourceManager};
use super::reloadable_resources::{ORDER_FUNCTION_LIBRARY, ReloadContext, ReloadListener, ReloadListenerRegistration, RELOAD_LISTENERS};
use crate::commands::functions::{self, CommandFunction};
use crate::commands::Commands;
use crate::registry::tags::{ElementRef, TagLoader};
use crate::registry::keys;
use crate::server::permissions::PermissionSet;
use indexmap::IndexMap;
use mc_core::ident::Identifier;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

/// `ServerFunctionLibrary.TYPE_KEY`.
pub const FUNCTION_REGISTRY_KEY: &str = "minecraft:function";

/// One loaded generation of functions and tags (the library's `volatile` maps); shared with the function manager.
#[derive(Default)]
pub struct FunctionLibraryData {
    functions: HashMap<Identifier, Arc<dyn CommandFunction>>,
    tags: IndexMap<Identifier, Vec<Arc<dyn CommandFunction>>>,
}

impl FunctionLibraryData {
    /// `getFunction(id)`.
    pub fn get_function(&self, id: &Identifier) -> Option<Arc<dyn CommandFunction>> {
        self.functions.get(id).cloned()
    }

    /// `getFunctions().keySet()`.
    pub fn function_names(&self) -> Vec<Identifier> {
        self.functions.keys().cloned().collect()
    }

    /// `getTag(tag)`: empty when the tag does not exist.
    pub fn get_tag(&self, tag: &Identifier) -> Vec<Arc<dyn CommandFunction>> {
        self.tags.get(tag).cloned().unwrap_or_default()
    }

    /// `getAvailableTags()`.
    pub fn available_tags(&self) -> Vec<Identifier> {
        self.tags.keys().cloned().collect()
    }
}

#[derive(Default)]
pub struct ServerFunctionLibrary {
    data: Arc<FunctionLibraryData>,
}

impl ServerFunctionLibrary {
    pub fn new() -> ServerFunctionLibrary {
        ServerFunctionLibrary::default()
    }

    pub fn data(&self) -> &Arc<FunctionLibraryData> {
        &self.data
    }

    /// `reload` (prepare + apply): compile every function, log the ones that fail, then build the tags over the survivors.
    fn load(manager: &dyn ResourceManager, commands: &Commands, permissions: &PermissionSet) -> FunctionLibraryData {
        let lister = FileToIdConverter::new("function", ".mcfunction");
        let compilation_context = Commands::create_compilation_context(permissions.clone());
        let mut functions: HashMap<Identifier, Arc<dyn CommandFunction>> = HashMap::new();
        for (resource_id, resource) in lister.list_matching_resources(manager) {
            let id = lister.file_to_id(&resource_id);
            let lines = match resource.open() {
                Ok(bytes) => read_lines(&String::from_utf8_lossy(&bytes)),
                Err(e) => {
                    log::error!("Failed to load function {id}\njava.util.concurrent.CompletionException: java.io.IOException: {e}");
                    continue;
                }
            };
            match functions::from_lines(id.clone(), commands.get_dispatcher(), &compilation_context, &lines) {
                Ok(function) => {
                    functions.insert(id, function);
                }
                Err(message) => {
                    // `CommandFunction.checkCommandLineLength` throws `IllegalStateException`, everything else `IllegalArgumentException`.
                    let class = if message.starts_with("Command too long: ") { "java.lang.IllegalStateException" } else { "java.lang.IllegalArgumentException" };
                    log::error!("Failed to load function {id}\njava.util.concurrent.CompletionException: {class}: {message}");
                }
            }
        }

        // Tags resolve their elements against the functions that just loaded: element ids are positions in `ordered`.
        let mut ordered: Vec<Arc<dyn CommandFunction>> = Vec::with_capacity(functions.len());
        let mut index_of: HashMap<Identifier, u32> = HashMap::with_capacity(functions.len());
        for (id, function) in &functions {
            index_of.insert(id.clone(), ordered.len() as u32);
            ordered.push(function.clone());
        }
        let mut lookup = |id: &Identifier, _required: bool| match index_of.get(id) {
            Some(index) => ElementRef::Found(*index),
            None => ElementRef::Missing,
        };
        let mut loader = TagLoader::new(&mut lookup, &keys::tags_dir_path(FUNCTION_REGISTRY_KEY));
        let builders = loader.load(manager);
        let tags = loader
            .build(&builders)
            .into_iter()
            .map(|(id, values)| (id, values.into_iter().map(|v| ordered[v as usize].clone()).collect()))
            .collect();
        FunctionLibraryData { functions, tags }
    }
}

/// `BufferedReader.lines()`: `\n`, `\r` and `\r\n` end a line; a trailing terminator does not start another one.
fn read_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find(['\n', '\r']) {
            Some(end) => {
                lines.push(rest[..end].to_owned());
                let skip = if rest[end..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[end + skip..];
            }
            None => {
                lines.push(rest.to_owned());
                rest = "";
            }
        }
    }
    lines
}

impl ReloadListener for ServerFunctionLibrary {
    fn name(&self) -> &'static str {
        "function_library"
    }

    fn reload(&mut self, ctx: &ReloadContext) -> Result<(), String> {
        self.data = Arc::new(Self::load(ctx.resource_manager, ctx.commands, ctx.function_compilation_permissions));
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[linkme::distributed_slice(RELOAD_LISTENERS)]
static FUNCTION_LIBRARY: ReloadListenerRegistration =
    ReloadListenerRegistration { name: "function_library", order: ORDER_FUNCTION_LIBRARY, create: || Box::new(ServerFunctionLibrary::new()) };

#[cfg(test)]
mod tests {
    use super::read_lines;

    #[test]
    fn lines_split_like_buffered_reader() {
        assert_eq!(read_lines(""), Vec::<String>::new());
        assert_eq!(read_lines("a\nb"), ["a", "b"]);
        assert_eq!(read_lines("a\r\nb\n"), ["a", "b"]);
        assert_eq!(read_lines("\n\n"), ["", ""]);
        assert_eq!(read_lines("a\rb"), ["a", "b"]);
    }
}
