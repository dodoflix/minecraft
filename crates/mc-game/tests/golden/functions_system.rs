//! The function system against a real `MinecraftServer` with the data pack `fixtures/commands_corpus/functions/fnpack` in
//! the world (the same pack the `functions.txt` corpus installs into the vanilla server): loading and its error logs,
//! `function` (all forms) / `return` / `schedule` / `/reload` / `/datapack`, chain limit, tick/load tags.

use crate::common;

use mc_core::ident::Identifier;
use mc_game::commands::{CommandResultCallback, CommandSource, CommandSourceStack, host};
use mc_game::server::MinecraftServer;
use mc_game::util::timer_queue::FunctionHost;
use mc_nbt::Tag;
use mc_text::Component;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Capture(Mutex<Vec<String>>);

impl CommandSource for Capture {
    fn send_system_message(&self, message: &Component) {
        self.0.lock().unwrap().push(message.get_string());
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

const PACK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/commands_corpus/functions/fnpack"
);

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// `command_common::test_server` with the function pack installed in the world's `datapacks` folder.
fn server_with_pack() -> (MinecraftServer, std::path::PathBuf, Box<dyn std::any::Any>) {
    let guard = crate::command_common::CWD_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use mc_game::server::dedicated::options::ServerOptions;
    use mc_game::server::dedicated::properties::DedicatedServerSettings;
    common::init_logger();
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("eula.txt"), "eula=true\n").unwrap();
    let pack_dir = dir.path().join("world/datapacks/fnpack");
    copy_dir(Path::new(PACK), &pack_dir);
    let options = ServerOptions {
        universe: dir.path().to_string_lossy().into_owned(),
        ..ServerOptions::default()
    };
    let settings =
        DedicatedServerSettings::new(dir.path().join("server.properties")).expect("settings");
    std::env::set_current_dir(dir.path()).expect("cd");
    mc_game::commands::BrigadierExceptions::install();
    let server = mc_game::server::dedicated::launch::create_server(options, settings)
        .expect("server")
        .expect("not stopped early");
    (server, pack_dir, Box::new((guard, dir)))
}

struct Outcome {
    messages: Vec<String>,
    results: Vec<(bool, i32)>,
}

fn run(server: &mut MinecraftServer, command: &str) -> Outcome {
    let capture = Arc::new(Capture::default());
    let results = Arc::new(Mutex::new(Vec::new()));
    let r = results.clone();
    let source: CommandSourceStack = server
        .create_command_source_stack()
        .with_source(capture.clone())
        .with_callback(CommandResultCallback::new(move |ok, v| {
            r.lock().unwrap().push((ok, v))
        }));
    let commands = server.commands.clone();
    commands.perform_prefixed_command(server, source, command);
    Outcome {
        messages: capture.0.lock().unwrap().clone(),
        results: results.lock().unwrap().clone(),
    }
}

fn storage(server: &mut MinecraftServer, id: &str, key: &str) -> Option<Tag> {
    host::command_storage_get(server, &Identifier::parse(id).unwrap())
        .get(key)
        .cloned()
}

fn has_log(logs: &[String], needle: &str) -> bool {
    logs.iter().any(|l| l.contains(needle))
}

#[test]
fn library_load_logs_the_broken_functions_and_tags() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::take_logs();
    let (server, _pack, _keep) = server_with_pack();
    let logs = common::take_logs();
    for needle in [
        "ERROR: Failed to load function fn:bad_syntax\njava.util.concurrent.CompletionException: java.lang.IllegalArgumentException: Whilst parsing command on line 2: Unknown or incomplete command",
        "ERROR: Failed to load function fn:bad_slash\njava.util.concurrent.CompletionException: java.lang.IllegalArgumentException: Unknown or invalid command '/say hi' on line 1 (did you mean 'say'? Do not use a preceding forwards slash.)",
        "ERROR: Failed to load function fn:bad_comment\njava.util.concurrent.CompletionException: java.lang.IllegalArgumentException: Unknown or invalid command '//say hi' on line 1 (if you intended to make a comment, use '#' not '//')",
        "ERROR: Failed to load function fn:bad_continuation\njava.util.concurrent.CompletionException: java.lang.IllegalArgumentException: Line continuation at end of file",
        "ERROR: Failed to load function fn:bad_macro\njava.util.concurrent.CompletionException: java.lang.IllegalArgumentException: Can't parse function line 1: 'say $(oops'",
        "Couldn't load tag fn:broken as it is missing following references: fn:does_not_exist (from file/fnpack)",
    ] {
        assert!(
            has_log(&logs, needle),
            "missing log line {needle:?} in {logs:#?}"
        );
    }
    let functions = server.functions();
    assert!(
        functions
            .get(&Identifier::parse("fn:hello").unwrap())
            .is_some()
    );
    assert!(
        functions
            .get(&Identifier::parse("fn:bad_syntax").unwrap())
            .is_none()
    );
    assert!(
        functions
            .get(&Identifier::parse("fn:macro").unwrap())
            .unwrap()
            .is_macro()
    );
    let all = functions.get_tag(&Identifier::parse("fn:all").unwrap());
    // `#fn:more` lists `fn:ret_value` twice: tags de-duplicate
    assert_eq!(
        all.iter().map(|f| f.id().to_string()).collect::<Vec<_>>(),
        ["fn:hello", "fn:nested", "fn:ret_value"]
    );
    assert!(
        functions
            .get_tag(&Identifier::parse("fn:broken").unwrap())
            .is_empty()
    );
    assert_eq!(
        functions
            .get_tag(&Identifier::parse("fn:optional").unwrap())
            .len(),
        1
    );
}

#[test]
fn load_and_tick_tags_run_once_after_load_then_every_tick() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut server, _pack, _keep) = server_with_pack();
    assert_eq!(storage(&mut server, "fn:state", "loaded"), None);
    server.tick_functions();
    assert_eq!(
        storage(&mut server, "fn:state", "loaded"),
        Some(Tag::Byte(1))
    );
    assert_eq!(
        storage(&mut server, "fn:state", "tick_ran"),
        Some(Tag::Byte(1))
    );
    run(&mut server, "data remove storage fn:state loaded");
    run(&mut server, "data remove storage fn:state tick_ran");
    server.tick_functions();
    assert_eq!(
        storage(&mut server, "fn:state", "loaded"),
        None,
        "the load tag runs once per (re)load"
    );
    assert_eq!(
        storage(&mut server, "fn:state", "tick_ran"),
        Some(Tag::Byte(1))
    );
}

#[test]
fn function_command_feedback_results_and_return_forms() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut server, _pack, _keep) = server_with_pack();
    server.tick_functions();

    let o = run(&mut server, "function fn:hello");
    assert_eq!(o.messages, ["Running function fn:hello"]);
    assert!(o.results.is_empty());
    assert_eq!(
        storage(&mut server, "fn:state", "hello"),
        Some(Tag::Byte(1))
    );

    let o = run(&mut server, "function fn:ret_value");
    assert_eq!(
        o.messages,
        [
            "Running function fn:ret_value",
            "Function fn:ret_value returned 7"
        ]
    );
    assert_eq!(o.results, [(true, 7)]);

    let o = run(&mut server, "function fn:ret_fail");
    assert_eq!(
        o.messages,
        [
            "Running function fn:ret_fail",
            "Function fn:ret_fail returned 0"
        ]
    );
    assert_eq!(o.results, [(false, 0)]);

    // `return` inside a nested function only leaves that function
    let o = run(&mut server, "function fn:calls_ret_three");
    assert_eq!(o.results, [(true, 9)]);

    // `return run function ...` returns the callee's result from the caller
    let o = run(&mut server, "function fn:ret_run");
    assert_eq!(o.results, [(true, 7)]);
    let o = run(&mut server, "function fn:ret_run_cmd");
    assert_eq!(o.results, [(true, 1)]);
    let o = run(&mut server, "function fn:ret_run_fail");
    assert_eq!(o.results, [(false, 0)]);

    // top-level `return` outside a function
    let o = run(&mut server, "return 5");
    assert_eq!(o.results, [(true, 5)]);
    let o = run(&mut server, "return fail");
    assert_eq!(o.results, [(false, 0)]);

    // tags: several functions, summed result; empty tag
    let o = run(&mut server, "function #fn:all");
    assert_eq!(
        o.messages[0],
        "Running functions fn:hello, fn:nested, fn:ret_value"
    );
    assert_eq!(o.results, [(true, 7)]);
    for tag in ["nothing", "broken", "missing_tag"] {
        let o = run(&mut server, &format!("function #fn:{tag}"));
        assert_eq!(
            o.messages,
            [format!("Can't find any functions for name fn:{tag}")]
        );
    }
}

#[test]
fn macro_functions_and_their_errors() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut server, _pack, _keep) = server_with_pack();
    server.tick_functions();

    let o = run(&mut server, "function fn:macro {a:1,b:\"x y\"}");
    assert_eq!(o.messages, ["Running function fn:macro"]);
    assert_eq!(
        storage(&mut server, "fn:out", "m"),
        Some(Tag::String("1".into()))
    );
    run(&mut server, "function fn:macro {a:1.5d,b:2b}");
    assert_eq!(
        storage(&mut server, "fn:out", "m"),
        Some(Tag::String("1.5".into()))
    );
    run(&mut server, "function fn:macro {a:[1,2],b:{k:\"v\"}}");
    assert_eq!(
        storage(&mut server, "fn:out", "m"),
        Some(Tag::String("[1,2]".into()))
    );

    // from storage
    run(&mut server, "data merge storage fn:args {a:\"q\",b:5s}");
    run(&mut server, "function fn:macro with storage fn:args");
    assert_eq!(
        storage(&mut server, "fn:out", "m"),
        Some(Tag::String("q".into()))
    );

    let failure = |server: &mut MinecraftServer, command: &str| run(server, command).messages;
    assert_eq!(
        failure(&mut server, "function fn:macro"),
        [
            "Running function fn:macro",
            "Failed to instantiate function fn:macro: Missing arguments to function fn:macro"
        ]
    );
    assert_eq!(
        failure(&mut server, "function fn:macro {a:1}"),
        [
            "Running function fn:macro",
            "Failed to instantiate function fn:macro: Missing argument b to function fn:macro"
        ]
    );
    assert_eq!(
        failure(&mut server, "function fn:macro with storage fn:nope"),
        [
            "Running function fn:macro",
            "Failed to instantiate function fn:macro: Missing argument a to function fn:macro"
        ]
    );
    assert_eq!(
        failure(
            &mut server,
            "function fn:macro with storage fn:state loaded"
        ),
        ["Invalid argument type: BYTE. Expected Compound"]
    );
    // a plain function ignores its arguments
    assert_eq!(
        failure(&mut server, "function fn:hello {a:1}"),
        ["Running function fn:hello"]
    );
    let parse = failure(&mut server, "function fn:macro_parse_error {c:\"bogus\"}");
    assert!(parse[1].starts_with("Failed to instantiate function fn:macro_parse_error: While instantiating macro fn:macro_parse_error: Command 'bogus' caused error: Unknown or incomplete command"), "{parse:?}");
}

#[test]
fn schedule_and_scheduled_events() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut server, _pack, _keep) = server_with_pack();
    let sched = |server: &mut MinecraftServer, c: &str| run(server, c);

    let o = sched(&mut server, "schedule function fn:sched 5t");
    assert_eq!(
        o.messages,
        ["Scheduled function 'fn:sched' in 5 tick(s) at gametime 5"]
    );
    assert_eq!(o.results, [(true, 5)]);
    assert_eq!(
        sched(&mut server, "schedule function fn:sched 0t").messages,
        ["Can't schedule for current tick"]
    );
    assert_eq!(
        sched(&mut server, "schedule function fn:macro 1t").messages,
        ["Can't schedule a macro"]
    );
    assert_eq!(
        sched(&mut server, "schedule function #fn:all 10t").messages,
        ["Scheduled tag 'fn:all' in 10 tick(s) at gametime 10"]
    );
    // `replace` (the default) drops the earlier event of the id; `append` keeps it
    sched(&mut server, "schedule function fn:sched 7t append");
    assert_eq!(server.scheduled_events().get_events_ids().count(), 2);
    sched(&mut server, "schedule function fn:sched 3t");
    assert_eq!(
        sched(&mut server, "schedule clear fn:sched").messages,
        ["Removed 1 schedule(s) with ID fn:sched"]
    );
    assert_eq!(
        sched(&mut server, "schedule clear fn:sched").messages,
        ["No schedules with ID fn:sched"]
    );
    // `schedule clear` reads an identifier: a tag id (`#...`) is a syntax error, tags cannot be cleared by name
    assert_eq!(sched(&mut server, "schedule clear #fn:all").results, []);

    // firing: due events leave the queue oldest first and run their function / tag as the game loop sender
    sched(&mut server, "schedule function fn:sched 2t");
    sched(&mut server, "schedule function #fn:all 4t");
    assert!(server.scheduled_events().pop_due(1).is_none());
    let callback = server.scheduled_events().pop_due(2).expect("due at 2");
    assert_eq!(storage(&mut server, "fn:state", "sched"), None);
    callback.handle(
        &mut server,
        &mut mc_game::util::timer_queue::TimerQueue::new(),
        2,
    );
    assert_eq!(
        storage(&mut server, "fn:state", "sched"),
        Some(Tag::Byte(1))
    );
    server.execute_function_tag(&Identifier::parse("fn:all").unwrap());
    assert_eq!(
        storage(&mut server, "fn:state", "hello"),
        Some(Tag::Byte(1))
    );
}

#[test]
fn chain_limit_stops_runaway_recursion() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut server, _pack, _keep) = server_with_pack();
    common::take_logs();
    run(&mut server, "function fn:loop");
    assert!(has_log(
        &common::take_logs(),
        "INFO: Command execution stopped due to limit (executed 65536 commands)"
    ));
    run(&mut server, "gamerule max_command_sequence_length 3");
    run(&mut server, "function fn:nested");
    assert!(has_log(
        &common::take_logs(),
        "INFO: Command execution stopped due to limit (executed 3 commands)"
    ));
}

#[test]
fn reload_picks_up_edited_functions_and_datapack_toggles_them() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut server, pack, _keep) = server_with_pack();
    server.tick_functions();
    std::fs::write(
        pack.join("data/fn/function/hello.mcfunction"),
        "data modify storage fn:state hello set value 2b\n",
    )
    .unwrap();
    std::fs::write(
        pack.join("data/fn/function/fresh.mcfunction"),
        "data modify storage fn:state fresh set value 1b\n",
    )
    .unwrap();
    assert!(
        run(&mut server, "function fn:fresh").messages[0].contains("fn:fresh"),
        "unknown before the reload"
    );
    let o = run(&mut server, "reload");
    assert_eq!(o.messages, ["Reloading!"]);
    run(&mut server, "function fn:hello");
    assert_eq!(
        storage(&mut server, "fn:state", "hello"),
        Some(Tag::Byte(2))
    );
    run(&mut server, "function fn:fresh");
    assert_eq!(
        storage(&mut server, "fn:state", "fresh"),
        Some(Tag::Byte(1))
    );
    // the load tag runs again at the next tick
    run(&mut server, "data remove storage fn:state loaded");
    server.tick_functions();
    assert_eq!(
        storage(&mut server, "fn:state", "loaded"),
        Some(Tag::Byte(1))
    );

    let o = run(&mut server, "datapack list enabled");
    assert!(
        o.messages[0].starts_with("There are 2 data pack(s) enabled: ")
            && o.messages[0].contains("[file/fnpack (world)]"),
        "{:?}",
        o.messages
    );
    assert_eq!(
        run(&mut server, "datapack disable \"file/fnpack\"").messages,
        ["Disabling data pack [file/fnpack (world)]"]
    );
    assert_eq!(
        run(&mut server, "datapack disable \"file/fnpack\"").messages,
        ["Pack 'file/fnpack' is not enabled!"]
    );
    assert!(run(&mut server, "function fn:hello").messages[0].contains("fn:hello"));
    assert_eq!(
        storage(&mut server, "fn:state", "hello"),
        Some(Tag::Byte(2))
    );
    run(&mut server, "data remove storage fn:state hello");
    run(&mut server, "function fn:hello");
    assert_eq!(
        storage(&mut server, "fn:state", "hello"),
        None,
        "functions of a disabled pack are gone"
    );
    assert_eq!(
        run(&mut server, "datapack enable \"file/fnpack\"").messages,
        ["Enabling data pack [file/fnpack (world)]"]
    );
    assert_eq!(
        run(&mut server, "datapack enable \"file/fnpack\"").messages,
        ["Pack 'file/fnpack' is already enabled!"]
    );
    run(&mut server, "function fn:hello");
    assert_eq!(
        storage(&mut server, "fn:state", "hello"),
        Some(Tag::Byte(2))
    );
    assert_eq!(
        run(&mut server, "datapack enable nope").messages,
        ["Unknown data pack 'nope'"]
    );
}
