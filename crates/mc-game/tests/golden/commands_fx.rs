//! `ParticleArgument` (options codecs) and the effect commands' parse errors against vanilla goldens
//! (`fixtures/commands/ParticleCorpus.java`, `ExceptionCorpus.java` over `fx_exception_corpus_commands.txt`).

use crate::command_common;

use mc_brigadier::{ImmutableStringReader, StringReader};
use mc_game::commands::CommandSourceStack;
use mc_game::commands::arguments::ParticleArgument;
use mc_game::server::permissions::PermissionSet;
use mc_protocol::Encode;

#[test]
fn particle_argument_matches_vanilla() {
    mc_game::commands::BrigadierExceptions::install();
    let mut checked = 0;
    for line in include_str!("../fixtures/commands/particle_corpus.txt").lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let input = fields[0];
        let mut reader = StringReader::new(input);
        let ours = match ParticleArgument::read_particle(&mut reader) {
            Ok(options) => {
                let mut wire = Vec::new();
                options.encode(&mut wire).unwrap();
                let hex: String = wire.iter().map(|b| format!("{b:02x}")).collect();
                format!(
                    "OK\t{hex}\t{}",
                    reader.get_remaining().encode_utf16().count()
                )
            }
            Err(e) => format!("ERR\t{}", e.get_message().replace('\n', "\\n")),
        };
        assert_eq!(ours, fields[1..].join("\t"), "input {input:?}");
        checked += 1;
    }
    assert!(checked > 80);
}

#[test]
fn effect_commands_fail_like_vanilla() {
    let (mut server, _world) = command_common::test_server();
    let commands = server.commands.clone();
    let dispatcher = commands.get_dispatcher();
    let source = CommandSourceStack::compilation_context(PermissionSet::AllPermissions);
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    let mut checked = 0;
    for line in include_str!("../fixtures/commands/fx_exception_corpus.txt").lines() {
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
    assert!(checked > 50);
}
