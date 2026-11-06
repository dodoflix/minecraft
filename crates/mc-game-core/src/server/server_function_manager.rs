//! `net.minecraft.server.ServerFunctionManager`: the `#minecraft:load` / `#minecraft:tick` function tags and running
//! functions as the server (`execute`). The state lives in [`ServerFunctionManager`]; everything that runs commands needs
//! the server and is a method of [`MinecraftServer`] (`tick_functions`, `execute_function`, ...).

use super::MinecraftServer;
use super::server_function_library::FunctionLibraryData;
use crate::commands::execution::ExecutionContext;
use crate::commands::functions::CommandFunction;
use crate::commands::{CommandResultCallback, CommandSourceStack, Commands, server_access};
use crate::server::permissions::LevelBasedPermissionSet;
use crate::util::timer_queue::{FunctionHost, TimerQueue};
use mc_core::ident::Identifier;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

fn tick_function_tag() -> Identifier {
    Identifier::parse("minecraft:tick").expect("static id")
}

fn load_function_tag() -> Identifier {
    Identifier::parse("minecraft:load").expect("static id")
}

pub struct ServerFunctionManager {
    ticking: Vec<Arc<dyn CommandFunction>>,
    post_reload: bool,
    library: Arc<FunctionLibraryData>,
}

impl ServerFunctionManager {
    /// `new ServerFunctionManager(server, library)`.
    pub fn new(library: Arc<FunctionLibraryData>) -> ServerFunctionManager {
        let mut manager = ServerFunctionManager {
            ticking: Vec::new(),
            post_reload: false,
            library: library.clone(),
        };
        manager.post_reload(library);
        manager
    }

    /// `replaceLibrary(library)`: the load tag runs again at the next tick.
    pub fn replace_library(&mut self, library: Arc<FunctionLibraryData>) {
        self.library = library.clone();
        self.post_reload(library);
    }

    fn post_reload(&mut self, library: Arc<FunctionLibraryData>) {
        self.ticking = library.get_tag(&tick_function_tag());
        self.post_reload = true;
    }

    /// `get(id)`.
    pub fn get(&self, id: &Identifier) -> Option<Arc<dyn CommandFunction>> {
        self.library.get_function(id)
    }

    /// `getTag(id)`.
    pub fn get_tag(&self, id: &Identifier) -> Vec<Arc<dyn CommandFunction>> {
        self.library.get_tag(id)
    }

    /// `getFunctionNames()`.
    pub fn get_function_names(&self) -> Vec<Identifier> {
        self.library.function_names()
    }

    /// `getTagNames()`.
    pub fn get_tag_names(&self) -> Vec<Identifier> {
        self.library.available_tags()
    }
}

impl MinecraftServer {
    /// `getFunctions()`.
    pub fn functions(&self) -> &ServerFunctionManager {
        &self.functions
    }

    /// `getDispatcher()` of the function manager = `getCommands()`.
    pub fn function_commands(&self) -> Arc<Commands> {
        self.commands.clone()
    }

    /// `ServerFunctionManager.tick`: the load tag once after a (re)load, then the tick tag, while the game runs normally.
    pub fn tick_functions(&mut self) {
        if self.tick_rate_manager.runs_normally() {
            if self.functions.post_reload {
                self.functions.post_reload = false;
                let functions = self.functions.get_tag(&load_function_tag());
                self.execute_tag_functions(&functions);
            }
            let ticking = self.functions.ticking.clone();
            self.execute_tag_functions(&ticking);
        }
    }

    fn execute_tag_functions(&mut self, functions: &[Arc<dyn CommandFunction>]) {
        for function in functions {
            let sender = self.get_game_loop_sender();
            self.execute_function(function, sender);
        }
    }

    /// `ServerFunctionManager.execute(function, sender)`.
    pub fn execute_function(
        &mut self,
        function: &Arc<dyn CommandFunction>,
        sender: CommandSourceStack,
    ) {
        let commands = self.commands.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let Ok(instantiated) = function.instantiate(None, commands.get_dispatcher()) else {
                // `FunctionInstantiationException`: swallowed.
                return;
            };
            server_access::enter_server(self, || {
                Commands::execute_command_in_context(&sender, |context: &mut ExecutionContext| {
                    context.queue_initial_function_call(
                        instantiated,
                        sender.clone(),
                        CommandResultCallback::EMPTY,
                    );
                });
            });
        }));
        if let Err(payload) = result {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "java.lang.RuntimeException".to_owned());
            log::warn!("Failed to execute function {}\n{message}", function.id());
        }
    }

    /// `getGameLoopSender()`.
    pub fn get_game_loop_sender(&self) -> CommandSourceStack {
        self.create_command_source_stack()
            .with_permission(LevelBasedPermissionSet::GAMEMASTER.into())
            .with_suppressed_output()
    }

    /// `getScheduledEvents()`.
    pub fn scheduled_events(&mut self) -> &mut TimerQueue<MinecraftServer> {
        self.saved_data
            .compute_if_absent(&self.saved_types.scheduled_events)
    }

    /// `getScheduledEvents().tick(this, gameTime)` as `ServerLevel.tickTime` calls it: the events run functions that act on the
    /// ticking level itself, so it is lent back to the server meanwhile. The queue cannot be lent to `handle` (it lives in the
    /// server), and the built-in callbacks never touch it; the queue reachable by commands is the real one.
    pub(super) fn tick_scheduled_events(
        &mut self,
        game_time: i64,
        level: &mut crate::server::level::ServerLevel,
    ) {
        self.with_lent_level(level, |server| {
            while let Some(callback) = server.scheduled_events().pop_due(game_time) {
                callback.handle(server, &mut TimerQueue::new(), game_time);
            }
        });
    }
}

impl FunctionHost for MinecraftServer {
    fn execute_function(&mut self, id: &Identifier) {
        if let Some(function) = self.functions.get(id) {
            let sender = self.get_game_loop_sender();
            MinecraftServer::execute_function(self, &function, sender);
        }
    }

    fn execute_function_tag(&mut self, id: &Identifier) {
        for function in self.functions.get_tag(id) {
            let sender = self.get_game_loop_sender();
            MinecraftServer::execute_function(self, &function, sender);
        }
    }
}
