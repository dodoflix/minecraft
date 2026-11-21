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
use mc_game::util::timer_queue::{FunctionHost, TimerQueue};
use mc_game::world::item::ItemStackExt;
use mc_protocol::codec::Encode;
use mc_protocol::components::ItemStack;
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
