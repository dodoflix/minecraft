//! Shared by the command tests.
#![allow(dead_code, reason = "each test target uses a subset of the shared helpers")]

/// The vanilla command tree: all features enabled like the golden generator (`createValidationContext`), built-in
/// registries plus empty data-driven ones (the tree only needs registry existence).
pub fn vanilla_commands() -> mc_game::commands::Commands {
    use mc_game::commands::{CommandSelection, Commands};
    mc_game::commands::BrigadierExceptions::install();
    Commands::new(CommandSelection::Dedicated, &vanilla_build_context())
}

/// The build context of [`vanilla_commands`].
pub fn vanilla_build_context() -> mc_game::commands::CommandBuildContext {
    use mc_game::commands::HolderLookupProvider as _;
    use mc_game::commands::CommandBuildContext;
    use mc_game::commands::command_build_context::StaticRegistries;
    use std::sync::Arc;

    struct WithEmptyDynamic;
    struct Empty;
    impl mc_game::commands::RegistryView for Empty {
        fn contains(&self, _: &mc_core::ident::Identifier) -> bool {
            false
        }
        fn element_ids(&self) -> Vec<mc_core::ident::Identifier> {
            Vec::new()
        }
        fn tag_ids(&self) -> Vec<mc_core::ident::Identifier> {
            Vec::new()
        }
        fn has_tag(&self, _: &mc_core::ident::Identifier) -> bool {
            false
        }
        fn tag_elements(&self, _: &mc_core::ident::Identifier) -> Option<Vec<mc_core::ident::Identifier>> {
            None
        }
    }
    impl mc_game::commands::HolderLookupProvider for WithEmptyDynamic {
        fn lookup(&self, key: &str) -> Option<Arc<dyn mc_game::commands::RegistryView>> {
            StaticRegistries.lookup(key).or_else(|| Some(Arc::new(Empty) as Arc<dyn mc_game::commands::RegistryView>))
        }
        fn list_registry_keys(&self) -> Vec<String> {
            StaticRegistries.list_registry_keys()
        }
    }
    CommandBuildContext::simple(Arc::new(WithEmptyDynamic), mc_world::level_storage::FeatureFlagSet::ALL)
}

/// Held by every test whose server lives in the working directory of the process (see [`test_server`]).
pub static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A real `MinecraftServer` over a fresh world in a temporary directory (nothing is started: no levels are loaded,
/// no listener, no console thread).
///
/// The user lists (`ops.json`, ...) live in the working directory like vanilla's: the process switches into the temp
/// directory, so tests holding a server are serialized (the second value keeps the lock and the directory alive).
pub fn test_server() -> (mc_game::server::MinecraftServer, Box<dyn std::any::Any>) {
    let guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use mc_game::server::dedicated::options::ServerOptions;
    use mc_game::server::dedicated::properties::DedicatedServerSettings;
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("eula.txt"), "eula=true\n").expect("eula");
    let options = ServerOptions { universe: dir.path().to_string_lossy().into_owned(), ..ServerOptions::default() };
    let settings = DedicatedServerSettings::new(dir.path().join("server.properties")).expect("settings");
    std::env::set_current_dir(dir.path()).expect("cd into the world directory");
    let server = mc_game::server::dedicated::launch::create_server(options, settings).expect("server").expect("not stopped early");
    (server, Box::new((guard, dir)))
}
