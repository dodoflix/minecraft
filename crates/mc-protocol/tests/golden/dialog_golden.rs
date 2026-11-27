//! Dialog codecs against vanilla's `Dialog.CODEC` / `STREAM_CODEC` / `CONTEXT_FREE_STREAM_CODEC`
//! (`fixtures/dialog/DialogGolden.java`, corpus `corpus.txt` = `gen_corpus.py`; regenerate with `fixtures/dialog/gen.sh`).
//! Covers every dialog of the vanilla data pack plus crafted valid/invalid cases over JSON and NBT ops: accept/reject,
//! rejection messages, the JSON / NBT re-encoding and the stream bytes.

use mc_core::ident::Identifier;
use mc_dfu::JSON;
use mc_nbt::{NbtAccounter, Tag, nbt_io};
use mc_protocol::components::Holder;
use mc_protocol::components::common::{dyn_to_serde, serde_to_dyn};
use mc_protocol::dialog::Dialog;
use mc_protocol::packets::play::ClientboundShowDialogPacket;
use mc_protocol::registry_context::registry_id;
use serde_json::Value;

const GOLDEN: &str = include_str!("../fixtures/dialog/golden.jsonl");

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn tag_from_hex(h: &str) -> Tag {
    nbt_io::read_unnamed_tag(
        &mut unhex(h).as_slice(),
        &mut NbtAccounter::unlimited_heap(),
    )
    .expect("fixture nbt")
}

fn same_key_order(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && same_key_order(va, vb))
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_key_order(p, q))
        }
        _ => true,
    }
}

/// `Holder<Dialog>` of the registry context as the packet's raw-id holder.
fn wire(h: &Holder<Dialog>) -> mc_protocol::codec::Holder<Dialog> {
    Dialog::wire_holder(h).expect("reference in the dialog registry")
}

fn stream(h: &Holder<Dialog>) -> Vec<u8> {
    mc_protocol::codec::to_bytes(&wire(h)).unwrap()
}

#[test]
fn dialogs_match_vanilla() {
    mc_protocol::components::install_text_hooks();
    let (mut ok_cases, mut err_cases, mut failures) = (0, 0, Vec::new());
    for line in GOLDEN.lines() {
        let rec: Value = serde_json::from_str(line).unwrap();
        let src = rec["src"].as_str().unwrap();
        let kind = rec["kind"].as_str().unwrap();
        let input = rec["input"].as_str().unwrap();
        let expect_ok = rec["ok"].as_bool().unwrap();
        let mut fail = |msg: String| failures.push(format!("{src}\n   {msg}"));
        match kind {
            "J" | "N" => {
                let decoded = if kind == "J" {
                    Dialog::codec()
                        .parse(&JSON, &serde_to_dyn(&serde_json::from_str(input).unwrap()))
                        .into_result()
                } else {
                    mc_dfu::parse_nbt(&Dialog::codec(), &tag_from_hex(input)).into_result()
                };
                let holder = match (decoded, expect_ok) {
                    (Ok(h), true) => h,
                    (Err(e), true) => {
                        fail(format!("vanilla accepted, we rejected: {e}"));
                        continue;
                    }
                    (Ok(h), false) => {
                        fail(format!(
                            "vanilla rejected ({}), we accepted: {h:?}",
                            rec["err"].as_str().unwrap()
                        ));
                        continue;
                    }
                    (Err(e), false) => {
                        err_cases += 1;
                        let expected = rec["err"].as_str().unwrap();
                        if e != expected {
                            fail(format!(
                                "error message\n      vanilla: {expected}\n      ours:    {e}"
                            ));
                        }
                        continue;
                    }
                };
                ok_cases += 1;
                let kind_name = if matches!(holder, Holder::Reference(_)) {
                    "reference"
                } else {
                    "direct"
                };
                if kind_name != rec["holder"].as_str().unwrap() {
                    fail(format!("holder kind {kind_name}"));
                }
                let expected_json: Value =
                    serde_json::from_str(rec["json"].as_str().unwrap()).unwrap();
                match Dialog::codec().encode_start(&JSON, &holder).into_result() {
                    Ok(d) => {
                        let got = dyn_to_serde(&d);
                        if got != expected_json || !same_key_order(&got, &expected_json) {
                            fail(format!(
                                "json\n      expected {expected_json}\n      got      {got}"
                            ));
                        }
                    }
                    Err(e) => fail(format!("json encode failed: {e}")),
                }
                let expected_nbt = tag_from_hex(rec["nbt"].as_str().unwrap());
                match mc_dfu::encode_nbt(&Dialog::codec(), &holder).into_result() {
                    Ok(t) if t == expected_nbt => {}
                    other => fail(format!(
                        "nbt\n      expected {expected_nbt}\n      got      {other:?}"
                    )),
                }
                let got = hex(&stream(&holder));
                if got != rec["stream"].as_str().unwrap() {
                    fail(format!(
                        "stream bytes\n      expected {}\n      got      {got}",
                        rec["stream"].as_str().unwrap()
                    ));
                }
                if let (Holder::Direct(d), Some(cf)) = (&holder, rec.get("cf")) {
                    let mut bytes = Vec::new();
                    d.encode_context_free(&mut bytes).unwrap();
                    let got = hex(&bytes);
                    if got != cf.as_str().unwrap() {
                        fail(format!(
                            "context-free bytes\n      expected {cf}\n      got      {got}"
                        ));
                    }
                }
            }
            "S" | "X" | "C" => {
                let bytes = unhex(input);
                let result: Result<String, String> = if kind == "C" {
                    mc_protocol::codec::from_bytes::<Dialog>(&bytes)
                        .map(|d| {
                            let mut out = Vec::new();
                            d.encode_context_free(&mut out).unwrap();
                            hex(&out)
                        })
                        .map_err(|e| e.to_string())
                } else {
                    mc_protocol::codec::from_bytes::<ClientboundShowDialogPacket>(&bytes)
                        .map(|p| hex(&mc_protocol::codec::to_bytes(&p).unwrap()))
                        .map_err(|e| e.to_string())
                };
                match (result, expect_ok) {
                    (Ok(re), true) => {
                        ok_cases += 1;
                        if re != rec["re"].as_str().unwrap() {
                            fail(format!(
                                "re-encoded\n      expected {}\n      got      {re}",
                                rec["re"].as_str().unwrap()
                            ));
                        }
                    }
                    (Err(e), true) => fail(format!("vanilla accepted, we rejected: {e}")),
                    (Ok(_), false) => fail(format!(
                        "vanilla rejected ({}), we accepted",
                        rec["err"].as_str().unwrap()
                    )),
                    (Err(_), false) => err_cases += 1,
                }
            }
            other => panic!("kind {other}"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s) ({ok_cases} ok / {err_cases} rejected passed):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every dialog of the vanilla data pack decodes, is the same value as its registered reference and has an id.
#[test]
fn vanilla_dialogs_are_registered() {
    let names: Vec<String> = mc_data::datapack::list("data/minecraft/dialog/")
        .filter_map(|p| {
            p.strip_prefix("data/minecraft/dialog/")?
                .strip_suffix(".json")
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(names.len(), 3);
    for name in names {
        let id = Identifier::parse(&format!("minecraft:{name}")).unwrap();
        assert!(registry_id("minecraft:dialog", &id).is_some(), "{id}");
        let json: Value = serde_json::from_slice(
            mc_data::datapack::read(&format!("data/minecraft/dialog/{name}.json"))
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        Dialog::direct_codec()
            .parse(&JSON, &serde_to_dyn(&json))
            .into_result()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// The four context-free component writes (disconnect reason, resource pack prompt, server data motd, display name of a
/// server link) write a dialog reference of a click event as the dialog; every other packet keeps the reference.
#[test]
fn context_free_component_fields_write_the_dialog() {
    use mc_protocol::codec::{Either, to_bytes};
    use mc_protocol::packets::common::{KnownLinkType, UntrustedEntry};
    use mc_protocol::packets::{configuration, play};
    use mc_text::Component;

    mc_protocol::components::install_text_hooks();
    let component = Component::from_json_str(
        r#"{"text":"x","click_event":{"action":"show_dialog","dialog":"minecraft:server_links"}}"#,
    )
    .unwrap();
    let encoded = |tag: &Tag| {
        let mut out = Vec::new();
        mc_protocol::codec::encode_tag(tag, &mut out).unwrap();
        out
    };
    let free = encoded(&component.to_nbt_context_free().unwrap());
    let registry_aware = encoded(&component.to_nbt().unwrap());
    assert_ne!(free, registry_aware, "the reference is expanded");
    assert_eq!(to_bytes(&component).unwrap(), registry_aware);

    let reason = play::ClientboundDisconnectPacket {
        reason: component.clone(),
    };
    assert_eq!(to_bytes(&reason).unwrap(), free);
    let reason = configuration::ClientboundDisconnectPacket {
        reason: component.clone(),
    };
    assert_eq!(to_bytes(&reason).unwrap(), free);

    let motd = play::ClientboundServerDataPacket {
        motd: component.clone(),
        icon_bytes: None,
    };
    assert_eq!(to_bytes(&motd).unwrap(), [free.clone(), vec![0]].concat());

    let pack = play::ClientboundResourcePackPushPacket {
        id: uuid::Uuid::nil(),
        url: "".into(),
        hash: "".into(),
        required: false,
        prompt: Some(component.clone()),
    };
    let bytes = to_bytes(&pack).unwrap();
    assert_eq!(&bytes[bytes.len() - free.len()..], free.as_slice());

    let custom = UntrustedEntry {
        link_type: Either::Right(component.clone()),
        link: "https://example.com".into(),
    };
    let bytes = to_bytes(&custom).unwrap();
    assert_eq!(bytes[0], 0);
    assert_eq!(&bytes[1..1 + free.len()], free.as_slice());
    let known = UntrustedEntry {
        link_type: Either::Left(KnownLinkType::BugReport),
        link: "https://example.com".into(),
    };
    assert_eq!(&to_bytes(&known).unwrap()[..2], &[1, 0]);
    // decoding takes the expanded dialog back as a direct value
    let back: UntrustedEntry = mc_protocol::codec::from_bytes(&bytes).unwrap();
    assert_eq!(back.link, custom.link);
}
