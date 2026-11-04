//! Execution engine + failure rendering with a mock `CommandSource`.

use mc_brigadier::arguments::integer;
use mc_brigadier::{CommandDispatcher, CommandSyntaxException, literal as lit};
use mc_game::commands::arguments::coordinates::Vec3Argument;
use mc_game::commands::{
    BrigadierExceptions, CommandResultCallback, CommandSource, CommandSourceStack, Commands, argument, literal,
};
use mc_game::server::MinecraftServer;
use mc_game::server::ServerShared;
use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionSet};
use mc_brigadier::ArgumentBuilder;
use mc_core::phys::{Vec2, Vec3};
use mc_text::Component;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MockSource {
    messages: Mutex<Vec<Component>>,
}

impl CommandSource for MockSource {
    fn send_system_message(&self, message: &Component) {
        self.messages.lock().unwrap().push(message.clone());
    }
    fn accepts_success(&self) -> bool {
        true
    }
    fn accepts_failure(&self) -> bool {
        true
    }
    fn should_inform_admins(&self) -> bool {
        false
    }
}

fn stack(source: Arc<MockSource>, level: i32) -> CommandSourceStack {
    CommandSourceStack::named(
        source,
        Vec3::new(1.0, 2.0, 3.0),
        Vec2::ZERO,
        mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        PermissionSet::LevelBased(LevelBasedPermissionSet::for_level(mc_game::server::permissions::PermissionLevel::by_id(level))),
        Component::literal("Mock"),
    )
}

fn server() -> MinecraftServer {
    let (shared, rx) = ServerShared::new();
    MinecraftServer::new(shared, rx)
}

fn texts(source: &MockSource) -> Vec<String> {
    source.messages.lock().unwrap().iter().map(|c| c.get_string()).collect()
}

fn commands(counter: Arc<Mutex<Vec<String>>>) -> Commands {
    let dispatcher: CommandDispatcher<CommandSourceStack> = CommandDispatcher::new();
    let c1 = counter.clone();
    dispatcher.register(literal("say").then(argument("n", integer()).executes(move |ctx| {
        let n = *ctx.get_argument::<i32>("n");
        c1.lock().unwrap().push(format!("say {n}"));
        ctx.get_source().send_success(|| Component::literal(format!("said {n}")), false);
        Ok(n)
    })));
    let c2 = counter.clone();
    dispatcher.register(literal("secret").requires(mc_game::commands::has_permission(mc_game::commands::LEVEL_GAMEMASTERS)).executes(move |_| {
        c2.lock().unwrap().push("secret".into());
        Ok(1)
    }));
    dispatcher.register(literal("pos").then(argument("p", Vec3Argument::vec3()).executes(|ctx| {
        let p = Vec3Argument::get_vec3(ctx, "p");
        ctx.get_source().send_success(|| Component::literal(format!("{} {} {}", p.x, p.y, p.z)), false);
        Ok(1)
    })));
    let root = dispatcher.get_root().clone();
    // `fanout` forks the source twice; `run` redirects back to the root.
    dispatcher.register(literal("fanout").fork(root.clone(), |ctx| Ok(vec![ctx.get_source().clone(), ctx.get_source().clone()])));
    dispatcher.register(lit("boom").executes(|_| panic!("kaboom")));
    Commands::from_dispatcher(dispatcher)
}

#[test]
fn executes_and_sends_feedback() {
    BrigadierExceptions::install();
    let log = Arc::new(Mutex::new(Vec::new()));
    let commands = commands(log.clone());
    let source = Arc::new(MockSource::default());
    let mut server = server();
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "/say 7");
    assert_eq!(*log.lock().unwrap(), vec!["say 7"]);
    assert_eq!(texts(&source), vec!["said 7"]);
}

#[test]
fn fork_runs_the_rest_per_source() {
    BrigadierExceptions::install();
    let log = Arc::new(Mutex::new(Vec::new()));
    let commands = commands(log.clone());
    let source = Arc::new(MockSource::default());
    let mut server = server();
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "fanout say 2");
    assert_eq!(*log.lock().unwrap(), vec!["say 2", "say 2"]);
}

#[test]
fn result_callback_receives_result() {
    BrigadierExceptions::install();
    let log = Arc::new(Mutex::new(Vec::new()));
    let commands = commands(log);
    let source = Arc::new(MockSource::default());
    let mut server = server();
    let results = Arc::new(Mutex::new(Vec::new()));
    let r = results.clone();
    let sender = stack(source, 4).with_callback(CommandResultCallback::new(move |ok, v| r.lock().unwrap().push((ok, v))));
    commands.perform_prefixed_command(&mut server, sender.clone(), "say 5");
    commands.perform_prefixed_command(&mut server, sender, "say x");
    assert_eq!(*results.lock().unwrap(), vec![(true, 5)]);
}

#[test]
fn errors_render_like_vanilla() {
    BrigadierExceptions::install();
    let commands = commands(Arc::default());
    let source = Arc::new(MockSource::default());
    let mut server = server();
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "nope");
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "say");
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "say 1 extra");
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 0), "secret");
    let t = texts(&source);
    // failure + context component per failure
    assert_eq!(t[0], "Unknown or incomplete command. See below for error");
    assert_eq!(t[1], "nope<--[HERE]");
    assert_eq!(t[2], "Unknown or incomplete command. See below for error");
    assert_eq!(t[3], "say<--[HERE]");
    assert_eq!(t[4], "Incorrect argument for command");
    assert_eq!(t[5], "say 1 extra<--[HERE]"); // cross-checked against vanilla in the golden corpus test
    assert_eq!(t[6], "Unknown or incomplete command. See below for error");
}

#[test]
fn panicking_command_reports_command_failed() {
    BrigadierExceptions::install();
    let commands = commands(Arc::default());
    let source = Arc::new(MockSource::default());
    let mut server = server();
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "boom");
    assert_eq!(texts(&source), vec!["An unexpected error occurred while trying to execute that command"]);
}

#[test]
fn coordinates_relative_to_source() {
    BrigadierExceptions::install();
    let commands = commands(Arc::default());
    let source = Arc::new(MockSource::default());
    let mut server = server();
    commands.perform_prefixed_command(&mut server, stack(source.clone(), 4), "pos ~1 ~ 0");
    assert_eq!(texts(&source), vec!["2 2 0.5"]);
    let _: Option<CommandSyntaxException> = None;
}
