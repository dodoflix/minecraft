//! The operating commands against a real `MinecraftServer` (fresh world, no levels started): feedback texts, state
//! changes and the files they write.

use crate::command_common;

use mc_game::commands::CommandSource;
use mc_game::server::MinecraftServer;
use mc_text::Component;
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

/// Runs `command` as the owner and returns the lines of feedback (failures included).
fn run(server: &mut MinecraftServer, command: &str) -> Vec<String> {
    mc_game::commands::BrigadierExceptions::install();
    let capture = Arc::new(Capture::default());
    let mut source = server.create_command_source_stack();
    source = source.with_source(capture.clone());
    let commands = server.commands.clone();
    commands.perform_prefixed_command(server, source, command);
    capture.0.lock().unwrap().clone()
}

#[test]
fn difficulty_gamerule_weather_seed_time() {
    let (mut server, _world) = command_common::test_server();
    assert_eq!(run(&mut server, "difficulty"), ["The difficulty is Easy"]);
    assert_eq!(
        run(&mut server, "difficulty hard"),
        ["The difficulty has been set to Hard"]
    );
    assert_eq!(
        run(&mut server, "difficulty hard"),
        ["The difficulty did not change; it is already set to Hard"]
    );

    assert_eq!(
        run(&mut server, "gamerule random_tick_speed"),
        ["Game rule random_tick_speed is currently set to 3"]
    );
    assert_eq!(
        run(&mut server, "gamerule random_tick_speed 7"),
        ["Game rule random_tick_speed is now set to 7"]
    );
    assert_eq!(
        run(&mut server, "gamerule minecraft:random_tick_speed 7"),
        ["Game rule random_tick_speed is already set to 7"]
    );
    assert_eq!(
        run(&mut server, "gamerule pvp false"),
        ["Game rule pvp is now set to false"]
    );

    assert_eq!(
        run(&mut server, "weather rain 100"),
        ["Set the weather to rain"]
    );
    assert_eq!(
        run(&mut server, "weather thunder"),
        ["Set the weather to rain and thunder"]
    );
    assert_eq!(
        run(&mut server, "weather clear 50"),
        ["Set the weather to clear"]
    );

    let seed = server.world_gen_settings.options.seed;
    assert_eq!(run(&mut server, "seed"), [format!("Seed: [{seed}]")]);

    assert_eq!(
        run(&mut server, "time query gametime"),
        ["The game time is 0 tick(s)"]
    );
    assert_eq!(
        run(&mut server, "time set noon"),
        ["Set minecraft:overworld to time marker minecraft:noon"]
    );
    assert_eq!(
        run(&mut server, "time set 5"),
        ["Set minecraft:overworld to 5 tick(s)"]
    );
    assert_eq!(
        run(&mut server, "time set 5"),
        ["Clock minecraft:overworld is already set to 5 tick(s)"]
    );
    assert_eq!(
        run(&mut server, "time add 10"),
        ["Set minecraft:overworld to 15 tick(s)"]
    );
    assert_eq!(
        run(&mut server, "time query day"),
        ["Timeline minecraft:day is at 15 tick(s)"]
    );
    assert_eq!(
        run(&mut server, "time pause"),
        ["Paused clock minecraft:overworld"]
    );
    assert_eq!(
        run(&mut server, "time pause"),
        ["Clock minecraft:overworld is already paused"]
    );
    assert_eq!(
        run(&mut server, "time resume"),
        ["Resumed clock minecraft:overworld"]
    );
    assert_eq!(
        run(&mut server, "time rate 2"),
        ["Clock minecraft:overworld will now advance at 2.0x normal rate"]
    );
    assert_eq!(
        run(&mut server, "time of minecraft:the_end query time"),
        ["Clock minecraft:the_end is at 0 tick(s)"]
    );
}

#[test]
fn lists_bans_whitelist_and_empty_targets() {
    let (mut server, _world) = command_common::test_server();
    assert_eq!(run(&mut server, "banlist"), ["There are no bans"]);
    assert_eq!(
        run(&mut server, "ban-ip 1.2.3.4 spam"),
        ["Banned IP 1.2.3.4: spam"]
    );
    assert_eq!(
        run(&mut server, "ban-ip 1.2.3.4"),
        ["Nothing changed. That IP is already banned"]
    );
    assert_eq!(
        run(&mut server, "ban-ip nobody"),
        ["Invalid IP address or unknown player"]
    );
    assert_eq!(
        run(&mut server, "banlist ips"),
        ["There are 1 ban(s):", "1.2.3.4 was banned by Server: spam"]
    );
    assert_eq!(
        run(&mut server, "pardon-ip 1.2.3.4"),
        ["Unbanned IP 1.2.3.4"]
    );
    assert_eq!(
        run(&mut server, "pardon-ip 1.2.3.4"),
        ["Nothing changed. That IP isn't banned"]
    );

    assert_eq!(
        run(&mut server, "whitelist list"),
        ["There are no whitelisted players"]
    );
    assert_eq!(
        run(&mut server, "whitelist off"),
        ["Whitelist is now turned off"]
    );
    assert_eq!(
        run(&mut server, "whitelist on"),
        ["Whitelist is now turned on"]
    );
    assert_eq!(
        run(&mut server, "whitelist on"),
        ["Whitelist is already turned on"]
    );
    assert_eq!(
        run(&mut server, "list"),
        ["There are 0 of a max of 20 players online: "]
    );

    for command in [
        "msg Bob hi",
        "tellraw @a \"x\"",
        "kick Bob",
        "op @a",
        "gamemode creative @a",
    ] {
        assert_eq!(
            run(&mut server, command),
            ["No player was found"],
            "{command}"
        );
    }
    assert_eq!(run(&mut server, "say hi"), Vec::<String>::new());
    assert_eq!(
        run(&mut server, "help time")[0],
        "/time set (<time>|<timemarker>)"
    );
}

#[test]
fn stop_halts_the_server() {
    let (mut server, _world) = command_common::test_server();
    assert_eq!(run(&mut server, "stop"), ["Stopping the server"]);
    assert!(!server.shared.is_running());
}
