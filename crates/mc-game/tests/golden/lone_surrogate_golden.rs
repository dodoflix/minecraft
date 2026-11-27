//! Items parsed by the item argument from SNBT strings with lone UTF-16 surrogates (`\ud800` escapes), against vanilla:
//! `fixtures/lone_surrogate.tsv`, recorded by `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`.
//! Per row the item as NBT (hex), as sent to a client (hex) and its hover name.

use mc_brigadier::{ArgumentType, CommandSyntaxException, StringReader};
use mc_core::java_string::JavaString;
use mc_dfu::NBT;
use mc_game::commands::CommandBuildContext;
use mc_game::commands::arguments::item::{
    ItemArgument, ItemInput, ItemPredicateArgument, ItemPredicateResult,
};
use mc_game::commands::command_build_context::AccessRegistries;
use mc_game::server::dedicated::java_properties::Properties;
use mc_game::server::dedicated::properties::DedicatedServerProperties;
use mc_game::server::network::hooks::LegacyServerInfo;
use mc_game::server::network::legacy_query::{
    create_legacy_disconnect_packet, create_version0_response, create_version1_response,
};
use mc_game::util::timer_queue::{FunctionHost, TimerQueue};
use mc_game::world::item::ItemStackExt;
use mc_protocol::codec::Encode;
use mc_protocol::components::ItemStack;
use mc_protocol::packets::configuration::ClientboundResourcePackPushPacket;
use mc_testkit::{hex, rows, unesc_units};
use std::sync::Arc;

use crate::command_common;

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

#[test]
fn items_from_commands_match_vanilla() {
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    mc_protocol::components::install_text_hooks();
    let (server, _keep) = command_common::test_server();
    let build = CommandBuildContext::simple(
        Arc::new(AccessRegistries(
            server.registries.composite_access().clone(),
        )),
        mc_world::level_storage::FeatureFlagSet::VANILLA,
    );
    let argument = ItemArgument::item(&build);
    let mut n = 0;
    for c in rows(GOLDEN, "item_cmd") {
        let command =
            String::from_utf16(&unesc_units(c[1])).expect("a command without a lone surrogate");
        let mut reader = StringReader::new(command.clone());
        let value = argument.parse(&mut reader).unwrap();
        let stack = value
            .downcast_ref::<ItemInput>()
            .unwrap()
            .create_item_stack(1)
            .unwrap();
        let nbt = ItemStack::codec()
            .encode_start(&NBT, &stack)
            .into_result()
            .unwrap();
        let mut tag = Vec::new();
        mc_nbt::nbt_io::write_any_tag(&nbt.to_tag().unwrap(), &mut tag).unwrap();
        assert_eq!(hex(&tag), c[2], "nbt of {command}");
        let mut wire = Vec::new();
        stack.encode(&mut wire).unwrap();
        assert_eq!(hex(&wire), c[3], "network bytes of {command}");
        let name = stack.get_hover_name().get_java_string();
        assert_eq!(
            name,
            JavaString::from_utf16_units(&unesc_units(c[4])),
            "hover name of {command}"
        );
        n += 1;
    }
    assert_eq!(n, 17);
}

/// An item argument that fails on a string with a lone surrogate: the exception message and the component built from its raw
/// message (what the client is sent) keep the unit.
fn assert_error_row(c: &[&str], error: CommandSyntaxException, command: &str) {
    let message = error.get_message();
    assert_eq!(
        message,
        JavaString::from_utf16_units(&unesc_units(c[2])),
        "message of {command}"
    );
    let mut out = Vec::new();
    let component = mc_game::commands::message::from_message(error.get_raw_message().as_ref());
    mc_nbt::nbt_io::write_any_tag(&component.to_nbt().unwrap(), &mut out).unwrap();
    assert_eq!(hex(&out), c[3], "component of {command}");
}

#[test]
fn item_argument_errors_match_vanilla() {
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    mc_protocol::components::install_text_hooks();
    let (server, _keep) = command_common::test_server();
    let build = CommandBuildContext::simple(
        Arc::new(AccessRegistries(
            server.registries.composite_access().clone(),
        )),
        mc_world::level_storage::FeatureFlagSet::VANILLA,
    );
    let item = ItemArgument::item(&build);
    let predicate = ItemPredicateArgument::item_predicate(&build);
    let mut n = 0;
    for c in rows(GOLDEN, "item_err") {
        let command =
            String::from_utf16(&unesc_units(c[1])).expect("a command without a lone surrogate");
        let error = match item.parse(&mut StringReader::new(command.clone())) {
            Err(e) => e,
            Ok(value) => value
                .downcast_ref::<ItemInput>()
                .unwrap()
                .create_item_stack(1)
                .err()
                .unwrap_or_else(|| panic!("{command} should fail")),
        };
        assert_error_row(&c, error, &command);
        n += 1;
    }
    for c in rows(GOLDEN, "item_pred_err") {
        let command =
            String::from_utf16(&unesc_units(c[1])).expect("a command without a lone surrogate");
        let error = predicate
            .parse(&mut StringReader::new(command.clone()))
            .err()
            .unwrap_or_else(|| panic!("{command} should fail"));
        assert_error_row(&c, error, &command);
        n += 1;
    }
    assert_eq!(n, 28);
}

/// Item predicates over books: a page, title or author with a lone surrogate matches only the same unit, never U+FFFD.
#[test]
fn book_predicates_match_vanilla() {
    mc_game::commands::BrigadierExceptions::install();
    mc_game::commands::install_text_validators();
    mc_protocol::components::install_text_hooks();
    let (server, _keep) = command_common::test_server();
    let build = CommandBuildContext::simple(
        Arc::new(AccessRegistries(
            server.registries.composite_access().clone(),
        )),
        mc_world::level_storage::FeatureFlagSet::VANILLA,
    );
    let item = ItemArgument::item(&build);
    let predicate = ItemPredicateArgument::item_predicate(&build);
    let mut n = 0;
    for c in rows(GOLDEN, "item_pred_match") {
        let text = |i: usize| String::from_utf16(&unesc_units(c[i])).unwrap();
        let (pred, book) = (text(1), text(2));
        let value = predicate
            .parse(&mut StringReader::new(pred.clone()))
            .unwrap();
        let stack = item
            .parse(&mut StringReader::new(book.clone()))
            .unwrap()
            .downcast_ref::<ItemInput>()
            .unwrap()
            .create_item_stack(1)
            .unwrap();
        let matched = value
            .downcast_ref::<ItemPredicateResult>()
            .unwrap()
            .test(&stack);
        assert_eq!(matched.to_string(), c[3], "{pred} on {book}");
        n += 1;
    }
    assert_eq!(n, 24);
}

struct NoFunctions;
impl FunctionHost for NoFunctions {
    fn execute_function(&mut self, _: &mc_core::ident::Identifier) {}
    fn execute_function_tag(&mut self, _: &mc_core::ident::Identifier) {}
}

/// The scheduled events saved data: an event id with a lone surrogate is kept, and is not the id with U+FFFD.
#[test]
fn scheduled_event_ids_match_vanilla() {
    let ty = TimerQueue::<NoFunctions>::type_();
    let mut n = 0;
    for c in rows(GOLDEN, "saved_timer") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        let tag = mc_nbt::snbt::parse(&input).unwrap();
        let queue = (ty.decode)(&tag).map_err(|e| e.error).unwrap();
        let mut bytes = Vec::new();
        mc_nbt::nbt_io::write_any_tag(&(ty.encode)(&queue).unwrap(), &mut bytes).unwrap();
        assert_eq!(hex(&bytes), c[2], "events of {input}");
        n += 1;
    }
    assert_eq!(n, 4);
}

/// `server.properties` text that spells `units` with `\uXXXX` escapes only.
fn escaped(units: &[u16]) -> String {
    units.iter().map(|u| format!("\\u{u:04x}")).collect()
}

/// `motd` of `server.properties` set to a string with lone surrogates (`\uXXXX` escapes in the file): the status component,
/// the legacy ping and the rewritten file.
#[test]
fn server_properties_motd_matches_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "props_motd") {
        let units = unesc_units(c[1]);
        let file = format!("motd={}\n", escaped(&units));
        let props = DedicatedServerProperties::new(Properties::load(&file).unwrap()).unwrap();
        let motd = props.motd.get().clone();
        assert_eq!(motd, JavaString::from_utf16_units(&units), "motd {}", c[1]);

        let status = mc_text::Component::literal(motd.clone());
        let mut out = Vec::new();
        mc_nbt::nbt_io::write_any_tag(&status.to_nbt().unwrap(), &mut out).unwrap();
        assert_eq!(hex(&out), c[2], "status component of {}", c[1]);

        assert_eq!(
            stored_props_file(&escaped(&units)),
            c[3],
            "stored file of {}",
            c[1]
        );

        let info = LegacyServerInfo {
            motd: motd.into_string_lossy(),
            player_count: 3,
            max_players: 20,
            server_version: "26.3".to_owned(),
        };
        let packets = format!(
            "{}|{}",
            hex(&create_legacy_disconnect_packet(&create_version0_response(
                &info
            ))),
            hex(&create_legacy_disconnect_packet(&create_version1_response(
                &info
            )))
        );
        assert_eq!(packets, c[4], "legacy ping of {}", c[1]);
        n += 1;
    }
    assert_eq!(n, 10);
}

/// The file `store` leaves for `motd=<value>`, see [`stored_props_line`].
fn stored_props_file(value: &str) -> String {
    stored_props_line(&format!("motd={value}"))
}

/// The file `store` leaves for the property line `line`: its length and SHA-256 with the date line replaced and every run
/// of `x` (the padding) replaced by its length plus the date's, as the recorded rows do.
fn stored_props_line(line: &str) -> String {
    use sha2::{Digest, Sha256};
    // the secret is random unless the file pins it
    let file = format!("{line}\nmanagement-server-secret={}\n", "k".repeat(40));
    let props = DedicatedServerProperties::new(Properties::load(&file).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.properties");
    props.store(&path);
    let text: String = std::fs::read(&path)
        .unwrap()
        .iter()
        .map(|&b| b as char)
        .collect();
    let date = text
        .split_once('\n')
        .and_then(|(_, rest)| rest.split_once('\n'))
        .map_or("", |(date, _)| date);
    let text = if date.is_empty() {
        text.clone()
    } else {
        text.replace(date, "<date>")
    };
    let mut normalized = String::new();
    let mut run = 0;
    for c in text.chars().chain(['\0']) {
        if c == 'x' {
            run += 1;
            continue;
        }
        if run >= 20 {
            normalized.push_str(&format!("x{{{}}}", run + date.len()));
        } else {
            normalized.extend(std::iter::repeat_n('x', run));
        }
        run = 0;
        normalized.push(c);
    }
    normalized.pop();
    let digest: Vec<u8> =
        Sha256::digest(normalized.chars().map(|c| c as u8).collect::<Vec<_>>()).to_vec();
    format!("{}:{}", normalized.chars().count(), hex(&digest))
}

/// `generator-settings` JSON that holds a lone surrogate (as a unit escape of the file or as a JSON escape): the object
/// keeps it, so the store fails.
#[test]
fn server_properties_generator_settings_match_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "props_generator") {
        let json = String::from_utf16(&unesc_units(c[1])).expect("the key is plain text");
        assert_eq!(
            stored_props_line(&format!("generator-settings={json}")),
            c[2],
            "store of {}",
            c[1]
        );
        n += 1;
    }
    assert_eq!(n, 4);
}

/// The file `store` leaves when the lone surrogate sits near a boundary of the writer's 8192-char buffer.
#[test]
fn server_properties_store_at_buffer_boundaries_matches_vanilla() {
    let plain = tempfile::tempdir().unwrap();
    let path = plain.path().join("server.properties");
    let props = DedicatedServerProperties::new(
        Properties::load(&format!(
            "motd=\nmanagement-server-secret={}\n",
            "k".repeat(40)
        ))
        .unwrap(),
    )
    .unwrap();
    props.store(&path);
    let text = std::fs::read_to_string(&path).unwrap();
    let offset = text.find("\nmotd=").unwrap() + 6;
    let mut n = 0;
    for c in rows(GOLDEN, "props_store_at") {
        let (at, units) = c[1].split_once(':').unwrap();
        let at: usize = at.parse().unwrap();
        let value = format!(
            "{}{}",
            "x".repeat(at - offset),
            escaped(&unesc_units(units))
        );
        assert_eq!(stored_props_file(&value), c[2], "store at {}", c[1]);
        n += 1;
    }
    assert_eq!(n, 35);
}

/// `level-seed` with lone surrogates: the hash of the trimmed string.
#[test]
fn server_properties_level_seed_matches_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "props_seed") {
        let file = format!("level-seed={}\n", escaped(&unesc_units(c[1])));
        let props = DedicatedServerProperties::new(Properties::load(&file).unwrap()).unwrap();
        assert_eq!(
            props.world_options.seed.to_string(),
            c[2],
            "seed of {}",
            c[1]
        );
        n += 1;
    }
    assert_eq!(n, 10);
}

/// `resource-pack-prompt` of `server.properties`: a prompt that decodes only partly is kept, one that does not parse is dropped.
#[test]
fn resource_pack_prompts_match_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "pack_prompt") {
        let prompt =
            String::from_utf16(&unesc_units(c[1])).expect("a prompt without a lone surrogate");
        let mut props = Properties::new();
        props.put("resource-pack", "http://localhost/pack.zip");
        props.put("resource-pack-id", "00000000-0000-0000-0000-000000000001");
        props.put(
            "resource-pack-sha1",
            "0123456789abcdef0123456789abcdef01234567",
        );
        props.put("resource-pack-prompt", prompt.as_str());
        let info = DedicatedServerProperties::new(props)
            .unwrap()
            .server_resource_pack_info
            .expect("a pack");
        let got = match info.prompt {
            None => "no prompt".to_owned(),
            Some(component) => {
                let mut out = Vec::new();
                mc_nbt::nbt_io::write_any_tag(&component.to_nbt().unwrap(), &mut out).unwrap();
                hex(&out)
            }
        };
        assert_eq!(got, c[2], "prompt {prompt}");
        n += 1;
    }
    assert_eq!(n, 15);
}

fn props_of(text: &str) -> DedicatedServerProperties {
    DedicatedServerProperties::new(Properties::load(text).unwrap()).unwrap()
}

fn units_of(cell: &str) -> JavaString {
    JavaString::from_utf16_units(&unesc_units(cell))
}

/// `level-type` is lower-cased and kept exactly, `rcon.password` is kept exactly and never equals what a client can send
/// (a decoded UTF-8 text has no lone surrogate), and a lone surrogate in either makes the store fail.
#[test]
fn level_type_and_rcon_password_match_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "props_level_type") {
        let file = escaped(&unesc_units(c[1]));
        let props = props_of(&format!("level-type={file}\n"));
        assert_eq!(
            props.world_dimension_data.level_type,
            units_of(c[2]),
            "level-type {}",
            c[1]
        );
        assert_eq!(
            stored_props_line(&format!("level-type={file}")),
            c[3],
            "store of level-type {}",
            c[1]
        );
        n += 1;
    }
    assert_eq!(n, 8);
    for c in rows(GOLDEN, "props_rcon") {
        let file = escaped(&unesc_units(c[1]));
        let password = props_of(&format!("rcon.password={file}\n")).rcon_password;
        assert_eq!(password, units_of(c[2]), "rcon.password {}", c[1]);
        assert_eq!(
            stored_props_line(&format!("rcon.password={file}")),
            c[3],
            "store of rcon.password {}",
            c[1]
        );
        assert_eq!((password == "\u{fffd}").to_string(), c[4], "FFFD {}", c[1]);
        assert_eq!((password == "?").to_string(), c[5], "? {}", c[1]);
        n += 1;
    }
    assert_eq!(n, 16);
}

/// The default id of a pack and the bytes of the push packet, or `no pack`.
fn pack_cells(props: &DedicatedServerProperties) -> String {
    let Some(info) = &props.server_resource_pack_info else {
        return "no pack".to_owned();
    };
    let mut out = Vec::new();
    ClientboundResourcePackPushPacket {
        id: info.id,
        url: info.url.clone(),
        hash: info.hash.clone(),
        required: info.is_required,
        prompt: info.prompt.clone(),
    }
    .encode(&mut out)
    .unwrap();
    format!("{}|{}", info.id, hex(&out))
}

/// `resource-pack`, `resource-pack-sha1`, the legacy `resource-pack-hash` and `resource-pack-prompt` with lone surrogates: the
/// default id (from the URL's UTF-8 bytes), the push packet and the store.
#[test]
fn resource_pack_properties_match_vanilla() {
    mc_protocol::components::install_text_hooks();
    let mut n = 0;
    for c in rows(GOLDEN, "props_pack_url") {
        let file = escaped(&unesc_units(c[1]));
        let line = format!("resource-pack={file}");
        assert_eq!(
            pack_cells(&props_of(&format!("{line}\n"))),
            c[2],
            "pack {}",
            c[1]
        );
        assert_eq!(stored_props_line(&line), c[3], "store of pack {}", c[1]);
        n += 1;
    }
    assert_eq!(n, 8);
    for c in rows(GOLDEN, "props_pack_hash") {
        let file = escaped(&unesc_units(c[1]));
        for (key, cells) in [("sha1", &c[2..4]), ("hash", &c[4..6])] {
            let lines = format!("resource-pack=http://x/p.zip\nresource-pack-{key}={file}");
            assert_eq!(
                pack_cells(&props_of(&format!("{lines}\n"))),
                cells[0],
                "{key} {}",
                c[1]
            );
            assert_eq!(
                stored_props_line(&lines),
                cells[1],
                "store of {key} {}",
                c[1]
            );
        }
        n += 1;
    }
    assert_eq!(n, 16);
    for c in rows(GOLDEN, "props_pack_prompt") {
        let lines = format!(
            "resource-pack=http://x/p.zip\nresource-pack-id=00000000-0000-0000-0000-000000000001\nresource-pack-prompt={}",
            escaped(&unesc_units(c[1]))
        );
        assert_eq!(
            pack_cells(&props_of(&format!("{lines}\n"))),
            c[2],
            "prompt {}",
            c[1]
        );
        assert_eq!(stored_props_line(&lines), c[3], "store of prompt {}", c[1]);
        n += 1;
    }
    assert_eq!(n, 24);
}
