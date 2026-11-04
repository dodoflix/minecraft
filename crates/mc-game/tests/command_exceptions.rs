//! The error messages of invalid commands against the real vanilla command tree
//! (`tests/fixtures/commands/ExceptionCorpus.java`): same text, same position, same context excerpt.

mod command_common;

use mc_game::commands::CommandSourceStack;
use mc_game::server::permissions::PermissionSet;

#[test]
fn invalid_commands_fail_like_vanilla() {
    let commands = command_common::vanilla_commands();
    let dispatcher = commands.get_dispatcher();
    let source = CommandSourceStack::compilation_context(PermissionSet::AllPermissions);
    let mut checked = 0;
    for line in include_str!("fixtures/commands/exception_corpus.txt").lines() {
        let (command, expected) = line.split_once('\t').expect("tab separated");
        let parse = dispatcher.parse(command, source.clone());
        let ours = match dispatcher.execute_parsed(&parse) {
            Ok(_) => "<no error>".to_owned(),
            Err(e) => e.get_message().replace('\n', "\\n"),
        };
        assert_eq!(ours, expected, "command {command:?}");
        checked += 1;
    }
    assert!(checked > 40);
}
