//! The error messages of invalid commands against the real vanilla command tree
//! (`tests/fixtures/commands/ExceptionCorpus.java`): same text, same position, same context excerpt.

use crate::command_common;

use mc_game::commands::CommandSourceStack;
use mc_game::server::permissions::PermissionSet;

#[test]
fn invalid_commands_fail_like_vanilla() {
    let (mut server, _world) = command_common::test_server();
    let commands = server.commands.clone();
    let dispatcher = commands.get_dispatcher();
    let source = CommandSourceStack::compilation_context(PermissionSet::AllPermissions);
    let mut checked = 0;
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    for line in include_str!("../fixtures/commands/exception_corpus.txt").lines() {
        let (command, expected) = line.split_once('\t').expect("tab separated");
        let ours = mc_game::commands::server_access::enter_server(&mut server, || {
            let parse = dispatcher.parse(command, source.clone());
            match dispatcher.execute_parsed(&parse) {
                Ok(_) => "<no error>".to_owned(),
                Err(e) => e.get_message().replace('\n', "\\n"),
            }
        });
        assert_eq!(ours, expected, "command {command:?}");
        checked += 1;
    }
    assert!(checked > 40);
}
